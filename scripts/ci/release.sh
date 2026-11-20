#!/usr/bin/env bash
set -euo pipefail

ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
cd "$ROOT"

: "${RELEASE_NOTES:?Set RELEASE_NOTES when starting the manual release pipeline}"

scripts/ci/verify.sh

version=$(python3 -c 'import tomllib; print(tomllib.load(open("Cargo.toml", "rb"))["package"]["version"])')
manifest_version=$(python3 -c 'import tomllib; print(tomllib.load(open("horizon-mod.toml", "rb"))["version"])')
sdk=$(python3 -c 'import tomllib; print(tomllib.load(open("horizon-mod.toml", "rb"))["native"]["horizon"])')
[[ "$version" == "$manifest_version" ]]

if gh release view "v$version" --repo JKearnsl/voice-chat >/dev/null 2>&1; then
    printf 'Release v%s already exists\n' "$version" >&2
    exit 1
fi

sha256=$(sha256sum target/horizon-mod/voice_chat.hmod | cut -d' ' -f1)
notes=$(mktemp)
trap 'rm -f "$notes"' EXIT
{
    printf '%s\n\n' "$RELEASE_NOTES"
    printf 'Built against Horizon SDK `%s`.\n\n' "$sdk"
    printf 'Verified with engine and Opus tests, Clippy, and package validation.\n'
    printf 'Live server and audio E2E verification must be completed before running this pipeline.\n\n'
    printf 'SHA-256: `%s`\n' "$sha256"
} > "$notes"

gh release create "v$version" target/horizon-mod/voice_chat.hmod \
    --repo JKearnsl/voice-chat --target "$(git rev-parse HEAD)" \
    --title "Voice Chat $version" --notes-file "$notes"
