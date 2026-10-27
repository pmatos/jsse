#!/usr/bin/env bash
# Run the real TypeScript compiler on jsse --node and diff it against Node (issue #1242).
#
# Usage:
#   TSC_DIR=/path/to/typescript ./scripts/run-tsc-oracle.sh [--full] [--no-timing] [--pin]
#
# TSC_DIR is an installed `typescript` package (the directory holding lib/_tsc.js and
# bin/tsc). Without it the script prints a notice and exits 0: the sources are not
# vendored (#1250 pins them later). Each scenario runs the same arguments through
# `lib/_tsc.js` on Node and on `jsse --node` in a fresh copy of
# scripts/tsc-fixtures/proj; stdout, exit code and the emitted files must be identical.
# Scenarios: --version, -v, --help, --all, an unknown flag, --init, --showConfig,
# a --noLib one-file emit and a --noLib compile with diagnostics. `bin/tsc` (the
# shebang entry that requires lib/tsc.js) is checked too. With --full, a compile
# against the real lib.d.ts runs as well (tens of seconds on jsse).
#
# Startup: after a warm-up, `--version` is run 10 times on jsse through lib/_tsc.js and
# bin/tsc and the median wall time of each must stay under
# TSC_STARTUP_BUDGET_MS (default 1000). --no-timing skips that check on loaded hosts.
#
# --pin rewrites scripts/tsc-fixtures/expected-sha256.txt from Node's output (needs
# TypeScript 6.0.3); tests/node_tsc.rs compares jsse against it without Node.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
JSSE="${JSSE:-$PROJECT_DIR/target/release/jsse}"
TREE="$PROJECT_DIR/scripts/tsc-fixtures/proj"
PINNED="$PROJECT_DIR/scripts/tsc-fixtures/expected-sha256.txt"
BUDGET_MS="${TSC_STARTUP_BUDGET_MS:-1000}"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

FULL=0
TIMING=1
PIN=0
for arg in "$@"; do
    case "$arg" in
        --full) FULL=1 ;;
        --no-timing) TIMING=0 ;;
        --pin) PIN=1 ;;
        *) echo "unknown option: $arg" >&2; exit 2 ;;
    esac
done

if [ -z "${TSC_DIR:-}" ]; then
    echo "SKIP: TSC_DIR is not set; point it at an installed typescript package (6.0.3) to run the tsc oracle"
    exit 0
fi
TSC_DIR="$(cd "$TSC_DIR" 2>/dev/null && pwd || echo "$TSC_DIR")"
TSC_JS="$TSC_DIR/lib/_tsc.js"
if [ ! -f "$TSC_JS" ]; then
    echo "SKIP: $TSC_JS does not exist (TSC_DIR must be the typescript package directory)"
    exit 0
fi
NODE_BIN="$(command -v node || true)"
if [ -z "$NODE_BIN" ]; then
    echo "SKIP: node not found; the tsc oracle needs it"
    exit 0
fi
if [ ! -x "$JSSE" ]; then
    echo "Building jsse (release)..."
    (cd "$PROJECT_DIR" && cargo build --release)
fi
TS_VERSION="$("$NODE_BIN" -p 'require(process.argv[1] + "/package.json").version' "$TSC_DIR")"
echo "typescript $TS_VERSION from $TSC_DIR (jsse: $JSSE)"
if [ "$TS_VERSION" != "6.0.3" ]; then
    echo "WARNING: the issue targets typescript 6.0.3; differences on $TS_VERSION are not necessarily jsse bugs" >&2
fi

# run <runner> <tag> <entry> args... : fresh tree, stdout/rc/emit digest into $WORK/<runner>.<tag>.*
run() {
    local runner="$1" tag="$2" entry="$3"
    shift 3
    local dir="$WORK/$runner.$tag.d"
    rm -rf "$dir"
    cp -r "$TREE" "$dir"
    local rc=0
    if [ "$runner" = node ]; then
        (cd "$dir" && "$NODE_BIN" "$entry" "$@") > "$WORK/$runner.$tag.out" 2> "$WORK/$runner.$tag.err" || rc=$?
    else
        (cd "$dir" && "$JSSE" --node "$entry" "$@") > "$WORK/$runner.$tag.out" 2> "$WORK/$runner.$tag.err" || rc=$?
    fi
    echo "exit $rc" >> "$WORK/$runner.$tag.out"
    (cd "$dir" && find . -type f | LC_ALL=C sort | while read -r f; do
        cmp -s "$f" "$TREE/$f" || sha256sum "$f"
    done) >> "$WORK/$runner.$tag.out"
}

FAIL=0
check() {
    local tag="$1"
    shift
    run node "$tag" "$@"
    run jsse "$tag" "$@"
    if diff -u "$WORK/node.$tag.out" "$WORK/jsse.$tag.out" > "$WORK/diff.txt"; then
        echo "ok   $tag"
    else
        echo "FAIL $tag: stdout, exit code or emit differs (--- node, +++ jsse)"
        head -40 "$WORK/diff.txt"
        head -5 "$WORK/jsse.$tag.err" | sed 's/^/  jsse stderr: /'
        FAIL=1
    fi
}

NOLIB=(--noLib --target es2020 --module commonjs --pretty false)
check version "$TSC_JS" --version
check short-version "$TSC_JS" -v
check help "$TSC_JS" --help
check all "$TSC_JS" --all
check bad-flag "$TSC_JS" --bogus
check init "$TSC_JS" --init
check show-config "$TSC_JS" --showConfig "${NOLIB[@]}" ok.ts
check nolib-emit "$TSC_JS" "${NOLIB[@]}" --outDir out ok.ts globals.d.ts
check nolib-diagnostics "$TSC_JS" "${NOLIB[@]}" --noEmit bad.ts globals.d.ts
check bin-version "$TSC_DIR/bin/tsc" --version
if [ "$FULL" -eq 1 ]; then
    check full-lib "$TSC_JS" --pretty false --outDir out --target es2020 --module commonjs ok.ts
    check full-lib-diagnostics "$TSC_JS" --pretty false --noEmit bad.ts
fi

if [ "$PIN" -eq 1 ]; then
    if [ "$TS_VERSION" != "6.0.3" ]; then
        echo "--pin needs TypeScript 6.0.3, found $TS_VERSION" >&2
        exit 2
    fi
    : > "$PINNED"
    for tag in version help all; do
        printf '%s %s\n' "$tag" "$(sed '/^exit /,$d' "$WORK/node.$tag.out" | sha256sum | cut -d' ' -f1)" >> "$PINNED"
    done
    echo "wrote $PINNED"
fi

if [ "$TIMING" -eq 1 ]; then
    for entry in "$TSC_JS" "$TSC_DIR/bin/tsc"; do
        if ! "$JSSE" --node "$entry" --version > /dev/null; then
            echo "FAIL startup: jsse --node ${entry#"$TSC_DIR"/} --version exited non-zero"
            FAIL=1
            continue
        fi
        samples=()
        for _ in 1 2 3 4 5 6 7 8 9 10; do
            t0=$(date +%s%N)
            "$JSSE" --node "$entry" --version > /dev/null
            t1=$(date +%s%N)
            samples+=("$(((t1 - t0) / 1000000))")
        done
        median=$(printf '%s\n' "${samples[@]}" | sort -n | sed -n 6p)
        echo "startup ${entry#"$TSC_DIR"/} --version (ms): ${samples[*]}  median $median  budget $BUDGET_MS"
        if [ "$median" -ge "$BUDGET_MS" ]; then
            echo "FAIL startup: median ${median} ms is not under ${BUDGET_MS} ms"
            FAIL=1
        fi
    done
fi

if [ "$FAIL" -ne 0 ]; then
    echo "tsc oracle: FAILED"
    exit 1
fi
echo "tsc oracle: all passed"
