#!/usr/bin/env bash
set -euo pipefail

if [[ -z "${GH_TOKEN:-}" ]]; then
    printf 'GH_TOKEN is unavailable; skipping GitHub commit status\n'
    exit 0
fi

state=${1:?Pass pending, success, failed, or canceled}
sha=${2:-${CI_COMMIT_SHA:?Set CI_COMMIT_SHA or pass a commit SHA}}
context="gitlab/${CI_JOB_NAME:-job}"
target_url=${CI_JOB_URL:-${CI_PIPELINE_URL:-https://github.com/JKearnsl/voice-chat}}

case "$state" in
    pending|running)
        state=pending
        description="GitLab job is running"
        ;;
    success)
        description="GitLab job passed"
        ;;
    failed|failure)
        state=failure
        description="GitLab job failed"
        ;;
    canceled)
        state=error
        description="GitLab job was canceled"
        ;;
    *)
        state=error
        description="GitLab job ended unexpectedly"
        ;;
esac

if ! gh api --method POST "repos/JKearnsl/voice-chat/statuses/$sha" \
    -f state="$state" \
    -f target_url="$target_url" \
    -f context="$context" \
    -f description="$description" \
    --silent; then
    printf 'Warning: could not publish GitHub commit status\n' >&2
fi
