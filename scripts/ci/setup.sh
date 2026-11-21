#!/usr/bin/env bash
set -euo pipefail

ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
cd "$ROOT"

project_pin=$(python3 -c 'import tomllib; print(tomllib.load(open("horizon-mod.toml", "rb"))["native"]["horizon"])')
request=${SDK_REQUEST:-$project_pin}
pin=$(python3 scripts/ci/resolve_sdk.py "$request")
[[ "$pin" =~ ^[0-9a-f]{40}$ ]]
mkdir -p "$ROOT/.horizon"
printf '%s\n' "$pin" > "$ROOT/.horizon/sdk-pin-$request"

export CARGO_HOME="${CARGO_HOME:-$HOME/.cargo}"
rustup target add wasm32-unknown-unknown
rustup component add rustfmt clippy
marker="$CARGO_HOME/.horizon-mod-pin"
current=""
if [[ -f "$marker" ]]; then
    current=$(<"$marker")
fi

if [[ ! -x "$CARGO_HOME/bin/cargo-horizon-mod" || "$current" != "$pin" ]]; then
    cargo install --force --locked --git https://github.com/horizon-rust/horizon --rev "$pin" \
        --target-dir "$CARGO_HOME/horizon-tooling/$pin" cargo-horizon-mod
    printf '%s\n' "$pin" > "$marker"
fi

[[ -x "$CARGO_HOME/bin/cargo-horizon-mod" ]]
cargo horizon-mod update --horizon "$project_pin"
cargo horizon-mod check
printf 'cargo-horizon-mod pinned to %s\n' "$pin"
