#!/usr/bin/env bash
# Run semgrep static analysis on the Rust source code.
#
# Usage:
#   ./scripts/semgrep.sh [--json]
#
# Requires: semgrep (pip install semgrep)
#
# By default uses the semgrep Rust ruleset. Pass --json for
# machine-readable JSON output (useful in CI).

set -euo pipefail

OUTPUT_FORMAT="text"
if [[ "${1:-}" == "--json" ]]; then
    OUTPUT_FORMAT="json"
fi

# Verify semgrep is available
if ! command -v semgrep &>/dev/null; then
    echo "Error: semgrep is not installed."
    echo "Install with: pip install semgrep  (or: pipx install semgrep)"
    exit 1
fi

echo "=== semgrep ==="

SEMGREP_ARGS=(
    --config auto
    --lang rust
    --error
    src/
)

if [[ "$OUTPUT_FORMAT" == "json" ]]; then
    SEMGREP_ARGS+=(--json)
fi

semgrep "${SEMGREP_ARGS[@]}"

echo ""
echo "Semgrep analysis passed."
