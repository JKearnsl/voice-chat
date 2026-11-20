#!/usr/bin/env python3
import re
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent


def replace_once(text, pattern, replacement, path):
    updated, count = re.subn(pattern, replacement, text, count=1, flags=re.MULTILINE)
    if count != 1:
        raise ValueError(f"Expected one package version in {path}")
    return updated


def main():
    with (ROOT / "Cargo.toml").open("rb") as file:
        current = tomllib.load(file)["package"]["version"]
    major, minor, patch = (int(part) for part in current.split("."))
    version = f"{major}.{minor}.{patch + 1}"

    replacements = {
        "Cargo.toml": (
            r'(^version = ")[^"]+("$)',
            rf"\g<1>{version}\2",
        ),
        "horizon-mod.toml": (
            r'(^version = ")[^"]+("$)',
            rf"\g<1>{version}\2",
        ),
        "Cargo.lock": (
            r'(^name = "voice_chat"\nversion = ")[^"]+("$)',
            rf"\g<1>{version}\2",
        ),
    }
    updated = {}
    for name, (pattern, replacement) in replacements.items():
        path = ROOT / name
        updated[path] = replace_once(path.read_text(), pattern, replacement, path)
    for path, text in updated.items():
        path.write_text(text)
    print(version)


if __name__ == "__main__":
    main()
