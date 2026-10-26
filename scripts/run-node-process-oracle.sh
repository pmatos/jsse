#!/usr/bin/env bash
# Cross-check the Node host `process` surface (issue #1215): jsse --node versus
# Node on argv, env, cwd/chdir and exitCode/exit.
#
# Usage:
#   ./scripts/run-node-process-oracle.sh [--node] [--no-cross-check]
#
# Each scripts/process-fixtures/*.fixture.js has header lines:
#   // RUN: <expected-exit-status> [script args...]   (one or more)
#   // ENV: K=V ...                                    (default: empty environment)
#   // CWD: tmp|repo                                   (default: tmp, with sub/inner and file.txt)
# The runner copies the fixture to one path (the prelude is embedded in
# `jsse --node`, nothing is prepended) and runs the same path with the
# identical argument tail on both engines under `env -i`. A run passes iff
# stdout and the exit status match Node's and the status equals the RUN line
# (stderr is not compared).
#
# Options:
#   --node             run on Node only (reference / debugging)
#   --no-cross-check   run on jsse only, compare against the RUN line alone

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
FIXTURE_DIR="$SCRIPT_DIR/process-fixtures"
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
if [ -z "$NODE_BIN" ] && [ "$CROSS_CHECK" -eq 1 ]; then
    echo "WARNING: node not found; running jsse only" >&2
    CROSS_CHECK=0
fi
if [ "$NODE_ONLY" -eq 0 ] && [ ! -x "$JSSE" ]; then
    echo "Building jsse (release)..."
    (cd "$PROJECT_DIR" && cargo build --release)
fi

TMPCWD="$WORK/cwd"
mkdir -p "$TMPCWD/sub/inner"
: > "$TMPCWD/file.txt"
BUNDLE="$WORK/bundle.js"

# <cwd> <env-words> <out-prefix> <engine...> -- <args...>; writes .out, sets RC
run_engine() {
    local cwd="$1" envs="$2" prefix="$3"
    shift 3
    RC=0
    # shellcheck disable=SC2086
    (cd "$cwd" && env -i PATH=/usr/bin:/bin $envs "$@" > "$prefix.out" 2> "$prefix.err") || RC=$?
}

FAIL=0
shopt -s nullglob
FIXTURES=("$FIXTURE_DIR"/*.fixture.js)
if [ "${#FIXTURES[@]}" -eq 0 ]; then
    echo "No fixtures found in $FIXTURE_DIR" >&2
    exit 2
fi

for fixture in "${FIXTURES[@]}"; do
    name="$(basename "$fixture" .fixture.js)"
    envs="$(sed -n 's|^// ENV: *||p' "$fixture" | head -1)"
    where="$(sed -n 's|^// CWD: *||p' "$fixture" | head -1)"
    cwd="$TMPCWD"
    [ "$where" = "repo" ] && cwd="$PROJECT_DIR"
    cp "$fixture" "$BUNDLE"
    n=0
    while IFS= read -r line; do
        n=$((n + 1))
        read -ra words <<< "$line"
        expect="${words[0]}"
        args=("${words[@]:1}")
        label="$name#$n [${args[*]:-}]"
        if [ "$NODE_ONLY" -eq 0 ]; then
            run_engine "$cwd" "$envs" "$WORK/jsse" "$JSSE" --node "$BUNDLE" "${args[@]}"
            jsse_rc="$RC"
            if [ "$jsse_rc" -ne "$expect" ]; then
                echo "FAIL $label: jsse exit $jsse_rc, expected $expect"
                sed 's/^/  stderr: /' "$WORK/jsse.err" | head -5
                FAIL=1
                continue
            fi
        fi
        if [ "$CROSS_CHECK" -eq 1 ] || [ "$NODE_ONLY" -eq 1 ]; then
            run_engine "$cwd" "$envs" "$WORK/node" "$NODE_BIN" "$BUNDLE" "${args[@]}"
            if [ "$RC" -ne "$expect" ]; then
                echo "FAIL $label: node exit $RC, expected $expect"
                FAIL=1
                continue
            fi
            if [ "$NODE_ONLY" -eq 0 ] && ! diff -u "$WORK/node.out" "$WORK/jsse.out" > "$WORK/diff.txt"; then
                echo "FAIL $label: stdout differs (--- node, +++ jsse)"
                cat "$WORK/diff.txt"
                FAIL=1
                continue
            fi
        fi
        echo "ok   $label (exit $expect)"
    done < <(sed -n 's|^// RUN: *||p' "$fixture")
done

if [ "$FAIL" -ne 0 ]; then
    echo "node-process oracle: FAILED"
    exit 1
fi
echo "node-process oracle: all passed"
