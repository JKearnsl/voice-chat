#!/usr/bin/env bash
set -euo pipefail

ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
cd "$ROOT"

project_pin=$(python3 -c 'import tomllib; print(tomllib.load(open("horizon-mod.toml", "rb"))["native"]["horizon"])')
request=${SDK_REQUEST:-$project_pin}
if [[ "$request" == nightly ]]; then
    if [[ -n "${GH_TOKEN:-}" ]]; then
        pin=$(gh release view nightly --repo horizon-rust/horizon --json targetCommitish --jq .targetCommitish)
    else
        pin=$(python3 - <<'PY'
import json
import urllib.request

request = urllib.request.Request(
    "https://api.github.com/repos/horizon-rust/horizon/releases/tags/nightly",
    headers={"User-Agent": "voice-chat-ci"},
)
with urllib.request.urlopen(request) as response:
    print(json.load(response)["target_commitish"])
PY
)
    fi
else
    pin=$request
fi
[[ "$pin" =~ ^[0-9a-f]{40}$ ]]

export CARGO_HOME="${CARGO_HOME:-$HOME/.cargo}"
rustup target add wasm32-unknown-unknown
rustup component add rustfmt clippy
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
cargo horizon-mod update --horizon "$project_pin"
cargo horizon-mod check
printf 'cargo-horizon-mod pinned to %s\n' "$pin"
