#!/usr/bin/env bash
# Run during provisioning, before making Git metadata read-only.
set -euo pipefail

if [[ $# -gt 1 || ( $# -eq 1 && "$1" != --check ) ]]; then
    echo "Usage: $0 [--check]" >&2
    exit 2
fi

repo_root=$(git -C "$(dirname "${BASH_SOURCE[0]}")/.." rev-parse --show-toplevel)
cd "$repo_root"

fail() {
    echo "Hook setup failed: $*" >&2
    echo "Do not commit or start feature implementation until hook setup succeeds. Provision hooks before mounting Git metadata read-only and provide a writable PRE_COMMIT_HOME cache." >&2
    exit 1
}

command -v pre-commit >/dev/null || fail "Install pre-commit first: uv tool install pre-commit"
[[ -z "${SKIP:-}" ]] || fail "Unset SKIP; required hooks must not be skipped."
if git config --get core.hooksPath >/dev/null; then
    fail "core.hooksPath is overridden; resolve it without disabling or overwriting existing hooks."
fi

hooks_installed() {
    local hook_type hook_path
    for hook_type in pre-commit commit-msg; do
        hook_path=$(git rev-parse --git-path "hooks/$hook_type")
        [[ -x "$hook_path" ]] && grep -q 'pre_commit' "$hook_path" || return 1
    done
}

if [[ "${1:-}" == --check ]]; then
    hooks_installed || fail "This worktree has no verified pre-commit and commit-msg hooks. Run ./scripts/ensure-hooks.sh first."
    exit 0
fi

if hooks_installed; then
    # A provisioned, read-only hooks directory needs no rewrite on resume.
    pre-commit install-hooks || fail "pre-commit could not prepare hook environments."
else
    # Do not use --overwrite: pre-commit preserves/chains existing Git hooks.
    pre-commit install --install-hooks || fail "pre-commit could not install the hooks or their environments."
fi
hooks_installed || fail "pre-commit and commit-msg must both be executable pre-commit hooks."
echo "Repository hooks and environments are installed and verified."
