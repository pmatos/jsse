#!/usr/bin/env bash
# Cross-check process.stdin/stdout/stderr, tty and readline (issue #1232):
# jsse --node versus Node over pipes, files and a pseudo-terminal.
#
# Usage:
#   ./scripts/run-node-tty-oracle.sh [--node] [--no-cross-check] [--update] [--only CASE]
#
# See scripts/tty-fixtures/oracle.py for the fixture format. Needs python3;
# the cross-check needs node. JSSE selects the binary (default target/release/jsse).

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
JSSE="${JSSE:-$PROJECT_DIR/target/release/jsse}"
if [ ! -x "$JSSE" ] && [[ " $* " != *" --node "* ]]; then
    echo "Building jsse (release)..."
    (cd "$PROJECT_DIR" && cargo build --release)
fi
exec python3 "$SCRIPT_DIR/tty-fixtures/oracle.py" --jsse "$JSSE" "$@"
