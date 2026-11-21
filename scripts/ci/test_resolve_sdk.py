import unittest
from unittest import mock

import resolve_sdk


SHA = "0" * 40
OTHER_SHA = "1" * 40
NEWER_SHA = "2" * 40


def release(tag, sha=SHA, *, name="", prerelease=True, game=True):
    return {
        "tag_name": tag,
        "name": name,
        "target_commitish": sha,
        "prerelease": prerelease,
        "assets": [{"name": resolve_sdk.GAME_ASSET}] if game else [],
    }


class StableSelectionTests(unittest.TestCase):
    def test_selects_newest_nightly_of_newest_week(self):
        releases = [
            release("nightly", NEWER_SHA, name="Nightly 2026-11-21 · 8f83e5c"),
            release("nightly-2026-11-20", OTHER_SHA),
            release("nightly-2026-11-13", SHA),
            release("nightly-2026-11-12", OTHER_SHA),
        ]

        selected = resolve_sdk.select_stable(releases)

        self.assertIs(selected, releases[0])

    def test_first_release_wins_when_dates_match(self):
        releases = [
            release("nightly-2026-11-20-aaaaaaa", NEWER_SHA),
            release("nightly-2026-11-20-bbbbbbb", OTHER_SHA),
        ]

        self.assertIs(resolve_sdk.select_stable(releases), releases[0])

    def test_real_stable_release_wins_over_nightlies(self):
        releases = [
            release("nightly", NEWER_SHA, name="Nightly 2026-11-21 · 8f83e5c"),
            release("v0.3.0", SHA, prerelease=False),
        ]

        self.assertIs(resolve_sdk.select_stable(releases), releases[1])

    def test_prereleases_and_builds_without_game_are_not_stable(self):
        releases = [
            release("v0.3.0", SHA, prerelease=True),
            release("nightly-2026-11-21", NEWER_SHA, game=False),
        ]

        with self.assertRaises(resolve_sdk.ResolutionError):
            resolve_sdk.select_stable(releases)

    def test_legacy_calendar_releases_are_not_new_stable_versions(self):
        releases = [
            release("v2026.10.16.01", SHA, prerelease=False),
            release("nightly", NEWER_SHA, name="Nightly 2026-11-21 · 8f83e5c"),
        ]

        self.assertIs(resolve_sdk.select_stable(releases), releases[1])


class ResolutionTests(unittest.TestCase):
    def test_full_sha_needs_no_network(self):
        with mock.patch.object(resolve_sdk, "github_json") as github:
            self.assertEqual(resolve_sdk.resolve(SHA), SHA)
        github.assert_not_called()

    def test_nightly_uses_its_release_commit(self):
        with mock.patch.object(
            resolve_sdk, "github_json", return_value=release("nightly", NEWER_SHA)
        ) as github:
            self.assertEqual(resolve_sdk.resolve("nightly"), NEWER_SHA)
        github.assert_called_once_with("releases/tags/nightly")

    def test_stable_resolves_release_pages(self):
        releases = [release("nightly", NEWER_SHA, name="Nightly 2026-11-21 · 8f83e5c")]
        with mock.patch.object(resolve_sdk, "github_json", return_value=releases) as github:
            self.assertEqual(resolve_sdk.resolve("stable"), NEWER_SHA)
        github.assert_called_once_with("releases", {"per_page": 100, "page": 1})

    def test_a_tag_is_resolved_to_a_full_sha(self):
        with mock.patch.object(resolve_sdk, "github_json", return_value={"sha": SHA}) as github:
            self.assertEqual(resolve_sdk.resolve_commit("v0.3.0"), SHA)
        github.assert_called_once_with("commits/v0.3.0")


class UpgradeTests(unittest.TestCase):
    def test_same_sha_is_accepted_without_network(self):
        with mock.patch.object(resolve_sdk, "github_json") as github:
            resolve_sdk.ensure_upgrade(SHA, SHA)
        github.assert_not_called()

    def test_descendant_is_accepted(self):
        with mock.patch.object(resolve_sdk, "github_json", return_value={"status": "ahead"}):
            resolve_sdk.ensure_upgrade(SHA, NEWER_SHA)

    def test_rollback_and_divergence_are_refused(self):
        for status in ["behind", "diverged"]:
            with self.subTest(status=status), mock.patch.object(
                resolve_sdk, "github_json", return_value={"status": status}
            ):
                with self.assertRaises(resolve_sdk.ResolutionError):
                    resolve_sdk.ensure_upgrade(SHA, NEWER_SHA)


if __name__ == "__main__":
    unittest.main()
