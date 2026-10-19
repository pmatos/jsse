#!/usr/bin/env bash
# Provision the selected worktree before starting an agent there.
set -euo pipefail

if [[ "${1:-}" == --worktree ]]; then
    if [[ $# -lt 3 ]]; then
        echo "Usage: $0 [--worktree <path>] <agent-command> [arguments...]" >&2
        exit 2
    fi
    cd "$2"
    shift 2
fi

if [[ $# -eq 0 ]]; then
    echo "Usage: $0 [--worktree <path>] <agent-command> [arguments...]" >&2
    exit 2
fi

repo_root=$(git rev-parse --show-toplevel)
cd "$repo_root"
"$repo_root/scripts/ensure-hooks.sh"
exec "$@"
