#!/usr/bin/env python3
import argparse
import datetime as dt
import json
import os
import re
import urllib.error
import urllib.parse
import urllib.request

API = "https://api.github.com/repos/horizon-rust/horizon"
GAME_ASSET = "horizon-linux-x86_64.tar.gz"
FULL_SHA = re.compile(r"[0-9a-f]{40}\Z")
NIGHTLY_DATE = re.compile(r"nightly-(\d{4}-\d{2}-\d{2})(?:-[0-9a-f]+)?\Z")
STABLE_VERSION = re.compile(r"v(\d+)\.(\d+)\.(\d+)(?:\.\d+)*\Z")


class ResolutionError(Exception):
    pass


def github_json(path, query=None):
    url = f"{API}/{path}"
    if query:
        url = f"{url}?{urllib.parse.urlencode(query)}"
    headers = {
        "Accept": "application/vnd.github+json",
        "User-Agent": "voice-chat-ci",
    }
    token = os.environ.get("GH_TOKEN")
    if token:
        headers["Authorization"] = f"Bearer {token}"
    request = urllib.request.Request(url, headers=headers)
    with urllib.request.urlopen(request, timeout=30) as response:
        return json.load(response)


def has_game(release):
    return any(asset.get("name") == GAME_ASSET for asset in release.get("assets", []))


def nightly_date(release):
    tag = release.get("tag_name", "")
    match = NIGHTLY_DATE.fullmatch(tag)
    if match:
        return match.group(1)
    if tag == "nightly":
        match = re.match(r"Nightly (\d{4}-\d{2}-\d{2})(?:\s|$)", release.get("name", ""))
        if match:
            return match.group(1)
    return None


def version_parts(tag):
    match = STABLE_VERSION.fullmatch(tag)
    if not match:
        return None
    parts = tuple(int(part) for part in match.groups())
    if parts[0] >= 1000:
        return None
    return parts


def select_stable(releases):
    for release in releases:
        tag = release.get("tag_name", "")
        version = version_parts(tag)
        if (
            version is not None
            and version > (0, 2, 97)
            and not release.get("prerelease", False)
            and nightly_date(release) is None
            and has_game(release)
        ):
            return release

    weekly = {}
    for position, release in enumerate(releases):
        if not has_game(release):
            continue
        date = nightly_date(release)
        if date is None:
            continue
        try:
            day = dt.date.fromisoformat(date)
        except ValueError:
            continue
        monday = day - dt.timedelta(days=day.weekday())
        candidate = (date, position, release)
        current = weekly.get(monday)
        if current is None or date > current[0]:
            weekly[monday] = candidate

    if not weekly:
        raise ResolutionError("No Horizon Stable build with a Linux x86_64 game was found")
    newest_week = max(weekly)
    return weekly[newest_week][2]


def resolve_commit(reference):
    if FULL_SHA.fullmatch(reference):
        return reference
    commit = github_json(f"commits/{urllib.parse.quote(reference, safe='')}")
    sha = commit.get("sha", "")
    if not FULL_SHA.fullmatch(sha):
        raise ResolutionError(f"Horizon reference {reference!r} did not resolve to a full commit SHA")
    return sha


def resolve(request):
    if FULL_SHA.fullmatch(request):
        return request
    if request == "nightly":
        release = github_json("releases/tags/nightly")
    elif request == "stable":
        releases = []
        for page in range(1, 6):
            batch = github_json("releases", {"per_page": 100, "page": page})
            releases.extend(batch)
            if len(batch) < 100:
                break
        release = select_stable(releases)
    else:
        raise ResolutionError(f"Unsupported SDK request {request!r}: use stable, nightly, or a full commit SHA")
    reference = release.get("target_commitish", "")
    if not reference:
        raise ResolutionError(f"Horizon release {release.get('tag_name', '')!r} has no target commit")
    return resolve_commit(reference)


def ensure_upgrade(base, candidate):
    if base == candidate:
        return
    if not FULL_SHA.fullmatch(base):
        raise ResolutionError("The current Horizon pin is not a full commit SHA")
    comparison = github_json(f"compare/{base}...{candidate}")
    status = comparison.get("status")
    if status != "ahead":
        raise ResolutionError(
            f"Refusing to move Horizon from {base} to {candidate}: comparison is {status or 'unknown'}"
        )


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("request")
    parser.add_argument("--base")
    args = parser.parse_args()
    try:
        candidate = resolve(args.request)
        if args.base:
            ensure_upgrade(args.base, candidate)
    except (ResolutionError, urllib.error.URLError, TimeoutError) as error:
        raise SystemExit(f"error: {error}") from error
    print(candidate)


if __name__ == "__main__":
    main()
