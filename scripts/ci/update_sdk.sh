#!/usr/bin/env bash
set -euo pipefail

ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
cd "$ROOT"

read_pin() {
    python3 -c 'import tomllib; print(tomllib.load(open("horizon-mod.toml", "rb"))["native"]["horizon"])'
}

old=$(read_pin)
cargo horizon-mod update --horizon "${SDK_REQUEST:-nightly}"
new=$(read_pin)
if [[ "$old" == "$new" ]]; then
    printf 'Voice Chat already targets %s\n' "$new"
    exit 0
fi

short=${new:0:7}
branch="fix/sdk-$short"
github=https://github.com/JKearnsl/voice-chat.git
if git ls-remote --exit-code --heads "$github" "$branch" >/dev/null 2>&1; then
    printf 'Branch %s already exists; skipping duplicate work\n' "$branch"
    exit 0
fi

version=$(python3 scripts/bump_patch.py)
scripts/ci/verify.sh

git switch -c "$branch"
git config user.name "${GIT_AUTHOR_NAME:-voice-chat-ci}"
git config user.email "${GIT_AUTHOR_EMAIL:-voice-chat-ci@localhost}"
git add Cargo.toml Cargo.lock horizon-mod.toml
git commit -m "fix: support Horizon SDK $short"

gh auth setup-git --hostname github.com
git push -u "$github" "$branch"

body=$(mktemp)
trap 'rm -f "$body"' EXIT
cat > "$body" <<EOF
## What changed

- Target Horizon SDK \`$new\`.
- Bump Voice Chat to \`$version\`.

## Verification

- Engine and Opus tests pass.
- Clippy passes with warnings denied.
- Package validation passes.
- The built \`.hmod\` is attached to the GitLab CI job.

No native Launcher build, live server test, or audio E2E test was performed. Complete those checks before merging and releasing.
EOF

gh pr create --repo JKearnsl/voice-chat --draft --base main --head "$branch" \
    --title "fix: support Horizon SDK $short" --body-file "$body"
