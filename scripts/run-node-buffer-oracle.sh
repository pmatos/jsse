#!/usr/bin/env bash
# Cross-check the embedded `buffer` and `string_decoder` modules (issue #1231)
# against Node on this host.
#
# Usage:
#   ./scripts/run-node-buffer-oracle.sh [--node] [--no-cross-check]
#
# scripts/buffer-fixtures/{buffer,sd}-probe.js print one line per observation:
# module shape and descriptors, argument errors, isUtf8/isAscii, the transcode
# matrix, Blob and File, and for StringDecoder every encoding over hand-picked and
# seeded random byte sequences cut at every point (partial multibyte characters,
# lone surrogates, odd base64 tails). jsse's stdout under `--node` must equal
# Node's byte for byte, and its last line, a digest, must equal the one pinned in
# scripts/buffer-fixtures/digests.txt. Pass "full" to a probe to print every case.
#
# Options:
#   --node             run on Node only (reference / regenerating digests.txt)
#   --no-cross-check   run on jsse only (exit 0 and the pinned digest required)

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
DIR="$PROJECT_DIR/scripts/buffer-fixtures"
JSSE="${JSSE:-$PROJECT_DIR/target/release/jsse}"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

NODE_ONLY=0
CROSS_CHECK=1
for arg in "$@"; do
    case "$arg" in
        --node) NODE_ONLY=1 ;;
        --no-cross-check) CROSS_CHECK=0 ;;
        *) echo "unknown option: $arg" >&2; exit 2 ;;
    esac
done

NODE_BIN="$(command -v node || true)"
if [ -z "$NODE_BIN" ] && { [ "$CROSS_CHECK" -eq 1 ] || [ "$NODE_ONLY" -eq 1 ]; }; then
    echo "WARNING: node not found; running jsse only" >&2
    CROSS_CHECK=0
    NODE_ONLY=0
fi
if [ "$NODE_ONLY" -eq 0 ] && [ ! -x "$JSSE" ]; then
    echo "Building jsse (release)..."
    (cd "$PROJECT_DIR" && cargo build --release)
fi

FAIL=0
for probe in buffer-probe sd-probe; do
    status="ok  "
    want="$(awk -v p="$probe" '$1 == p { print $2 " " $3 }' "$DIR/digests.txt")"
    if [ "$NODE_ONLY" -eq 0 ]; then
        for flags in "" "--no-bytecode"; do
            rc=0
            # shellcheck disable=SC2086
            "$JSSE" --node $flags "$DIR/$probe.js" > "$WORK/jsse.out" 2> "$WORK/jsse.err" || rc=$?
            if [ "$rc" -ne 0 ]; then
                echo "FAIL $probe ${flags:-default}: jsse --node exit $rc"
                head -5 "$WORK/jsse.err" | sed 's/^/  stderr: /'
                status="FAIL"; FAIL=1
            elif [ "$(tail -n 1 "$WORK/jsse.out")" != "$want" ]; then
                echo "FAIL $probe ${flags:-default}: digest '$(tail -n 1 "$WORK/jsse.out")' != pinned '$want'"
                status="FAIL"; FAIL=1
            fi
        done
    fi
    if [ "$CROSS_CHECK" -eq 1 ] || [ "$NODE_ONLY" -eq 1 ]; then
        rc=0
        "$NODE_BIN" "$DIR/$probe.js" > "$WORK/node.out" 2> "$WORK/node.err" || rc=$?
        if [ "$rc" -ne 0 ]; then
            echo "FAIL $probe: node exit $rc"
            head -5 "$WORK/node.err" | sed 's/^/  stderr: /'
            status="FAIL"; FAIL=1
        else
            if [ "$(tail -n 1 "$WORK/node.out")" != "$want" ]; then
                echo "FAIL $probe: node digest '$(tail -n 1 "$WORK/node.out")' != pinned '$want'"
                status="FAIL"; FAIL=1
            fi
            if [ "$NODE_ONLY" -eq 0 ] && [ "$status" != "FAIL" ]; then
                if ! diff -u "$WORK/node.out" "$WORK/jsse.out" > "$WORK/diff.txt"; then
                    echo "FAIL $probe: stdout differs (--- node, +++ jsse)"
                    head -40 "$WORK/diff.txt"
                    status="FAIL"; FAIL=1
                fi
            fi
        fi
    fi
    echo "$status $probe"
done

if [ "$FAIL" -ne 0 ]; then
    echo "node-buffer oracle: FAILED"
    exit 1
fi
echo "node-buffer oracle: all passed"
