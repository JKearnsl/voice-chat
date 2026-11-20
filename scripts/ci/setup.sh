#!/usr/bin/env bash
set -euo pipefail

: "${GH_TOKEN:?Set GH_TOKEN to a GitHub token with contents and pull requests write access}"

ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
cd "$ROOT"

pin=$(gh release view nightly --repo horizon-rust/horizon --json targetCommitish --jq .targetCommitish)
[[ "$pin" =~ ^[0-9a-f]{40}$ ]]

export CARGO_HOME="${CARGO_HOME:-$HOME/.cargo}"
marker="$CARGO_HOME/.horizon-mod-pin"
current=""
if [[ -f "$marker" ]]; then
    current=$(<"$marker")
fi

if [[ ! -x "$CARGO_HOME/bin/cargo-horizon-mod" || "$current" != "$pin" ]]; then
    cargo install --force --locked --git https://github.com/horizon-rust/horizon --rev "$pin" cargo-horizon-mod
    printf '%s\n' "$pin" > "$marker"
fi

[[ -x "$CARGO_HOME/bin/cargo-horizon-mod" ]]
project_pin=$(python3 -c 'import tomllib; print(tomllib.load(open("horizon-mod.toml", "rb"))["native"]["horizon"])')
cargo horizon-mod update --horizon "$project_pin"
printf 'cargo-horizon-mod pinned to %s\n' "$pin"
