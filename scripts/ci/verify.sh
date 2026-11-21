#!/usr/bin/env bash
set -euo pipefail

ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
cd "$ROOT"

trap 'rm -f src/opus-rs/Cargo.lock' EXIT
cargo fetch --locked
cargo fmt --check
cargo test --offline
cargo test --manifest-path src/opus-rs/Cargo.toml --offline
cargo clippy --offline --all-targets -- -D warnings
cargo horizon-mod check
cargo horizon-mod build
cargo horizon-mod check target/horizon-mod/voice_chat.hmod --json
sha256sum target/horizon-mod/voice_chat.hmod
git diff --check
