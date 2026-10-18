#!/usr/bin/env bash
# Claude PreToolUse: exit 2 blocks the tool when the worktree is not ready.
set -euo pipefail

repo_root=$(git rev-parse --show-toplevel) || exit 2
if ! "$repo_root/scripts/ensure-hooks.sh" --check >&2; then
    echo "Feature work blocked: install and verify hooks in this worktree before editing files or running shell commands. Provision read-only workspaces from the host before starting the agent." >&2
    exit 2
fi
