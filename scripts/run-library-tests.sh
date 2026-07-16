#!/usr/bin/env bash
# Run a real-world npm library's own test suite on jsse.
#
# Usage:
#   ./scripts/run-library-tests.sh <lib> [--clean] [--node] [--no-cross-check]
#
# For each library there is a config at scripts/libs/<lib>.sh describing where
# to clone it, which ref to pin, how to prepare it, which file is the bundle
# entry, and how to read the pass/fail verdict from its output. The runner:
#
#   1. clones the pinned ref (cached under /tmp/jsse-libtests/<lib>/repo),
#   2. runs the library's prepare hook (npm install / build / patches),
#   3. bundles the entry with a pinned esbuild into a single IIFE,
#   4. prepends the shared Node-global shims (node-shim.js + node-buffer-shim.js;
#      test262 never sees them),
#   5. runs the bundle on jsse (release) and reports the verdict.
#
# By default it also runs the same bundle on Node as a reference oracle and
# requires the two engines to agree on the test count — this closes the
# "jsse silently ran fewer tests but still self-reported X of X" false pass.
#
# Options:
#   --clean            wipe this library's cache and rebuild from scratch
#   --node             run on Node only (reference / debugging), skip jsse
#   --no-cross-check   run on jsse only, skip the Node count cross-check

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
LIBS_DIR="$SCRIPT_DIR/libs"
CACHE_ROOT="/tmp/jsse-libtests"
JSSE="$PROJECT_DIR/target/release/jsse"

# Pinned esbuild so every library bundles identically and reproducibly (the
# per-repo `npx esbuild` this replaces silently tracked npx-latest). The
# tooling dir is version-keyed: a plain `[ -x esbuild ]` existence check can't
# tell versions apart, so bumping ESBUILD_VERSION must land in a fresh dir
# rather than silently reuse a cached older binary. (`--clean` only wipes the
# per-library cache, not this shared tooling dir.)
ESBUILD_VERSION="0.25.0"
TOOLING_DIR="$CACHE_ROOT/tooling/$ESBUILD_VERSION"

# ---- argument parsing ------------------------------------------------------
LIB=""
CLEAN=0
NODE_ONLY=0
CROSS_CHECK=1
for arg in "$@"; do
    case "$arg" in
        --clean) CLEAN=1 ;;
        --node) NODE_ONLY=1 ;;
        --no-cross-check) CROSS_CHECK=0 ;;
        --*) echo "unknown option: $arg" >&2; exit 2 ;;
        *) LIB="$arg" ;;
    esac
done

if [ -z "$LIB" ]; then
    echo "usage: $(basename "$0") <lib> [--clean] [--node] [--no-cross-check]" >&2
    echo "available libs:" >&2
    for f in "$LIBS_DIR"/*.sh; do [ -e "$f" ] && echo "  $(basename "$f" .sh)" >&2; done
    exit 2
fi

CONFIG="$LIBS_DIR/$LIB.sh"
if [ ! -f "$CONFIG" ]; then
    echo "no config for library '$LIB' (expected $CONFIG)" >&2
    exit 2
fi

# ---- config defaults (a config may override any of these) ------------------
LIB_ESBUILD_PLATFORM="node"
LIB_ESBUILD_EXTRA=()
LIB_SHIM=""
LIB_SHIMS=()
LIB_ENV=()
LIB_EXPECT_COUNT=""   # if set, both engines must report exactly this count
LIB_TIMEOUT=""        # if set (seconds), wrap each engine run so a hang/slow
                      # suite reports cleanly instead of blocking the caller

# lib_prepare runs (cd'd into the cloned repo) once, after clone. Default: none.
lib_prepare() { :; }

# lib_verdict <output_file> <exit_code> must echo "PASS <n>" or "FAIL <n>"
# (n = the test count to cross-check between engines) and return 0/1 to match.
# Default: succeed iff the engine exited 0 (n unknown → 0).
lib_verdict() {
    if [ "$2" -eq 0 ]; then echo "PASS 0"; return 0; fi
    echo "FAIL 0"; return 1
}

# Shared verdict helper for suites that end with "In total, X of Y tests
# passed" (the MikeMcl decimal.js / big.js / bignumber.js family). PASS iff
# X == Y and Y > 0; the cross-checked count is Y.
verdict_in_total() {
    local out="$1" line X Y
    line="$(grep -oE 'In total, [0-9]+ of [0-9]+ tests passed' "$out" 2>/dev/null | tail -1 || true)"
    if [ -z "$line" ]; then echo "FAIL 0"; return 1; fi
    if [[ "$line" =~ ([0-9]+)\ of\ ([0-9]+) ]]; then
        X="${BASH_REMATCH[1]}"; Y="${BASH_REMATCH[2]}"
        if [ "$X" -eq "$Y" ] && [ "$Y" -gt 0 ]; then
            echo "PASS $Y"; return 0
        fi
        echo "FAIL $Y"; return 1
    fi
    echo "FAIL 0"; return 1
}

# shellcheck source=/dev/null
source "$CONFIG"

: "${LIB_REPO:?config must set LIB_REPO}"
: "${LIB_REF:?config must set LIB_REF (pin a tag/branch/sha)}"
: "${LIB_ENTRY:?config must set LIB_ENTRY (bundle entry file)}"

# ---- cache layout ----------------------------------------------------------
LIB_CACHE="$CACHE_ROOT/$LIB"
REPO_DIR="$LIB_CACHE/repo"
BUNDLE="$LIB_CACHE/bundle.js"
FINAL="$LIB_CACHE/final.js"
PREPARED_MARKER="$LIB_CACHE/.prepared"

if [ "$CLEAN" -eq 1 ]; then
    echo "Cleaning cache for $LIB..."
    rm -rf "$LIB_CACHE"
fi
mkdir -p "$LIB_CACHE"

# ---- step 1: pinned esbuild ------------------------------------------------
ESBUILD="$TOOLING_DIR/node_modules/.bin/esbuild"
if [ ! -x "$ESBUILD" ]; then
    echo "Installing pinned esbuild@$ESBUILD_VERSION..."
    mkdir -p "$TOOLING_DIR"
    printf '{ "private": true }\n' > "$TOOLING_DIR/package.json"
    (cd "$TOOLING_DIR" && npm install --silent "esbuild@$ESBUILD_VERSION")
fi

# ---- step 2: clone (pinned, cached) ----------------------------------------
if [ ! -d "$REPO_DIR" ]; then
    echo "Cloning $LIB ($LIB_REPO @ $LIB_REF)..."
    # A shallow --branch clone is fastest, but `git clone --branch` only accepts
    # a branch/tag name — a commit-SHA pin fails with "Remote branch <sha> not
    # found". LIB_REF advertises tag/branch/sha, so fall back to a full clone +
    # detached checkout of the exact revision when the shallow clone can't
    # resolve the ref (i.e. a SHA pin).
    if ! git clone --depth 1 --branch "$LIB_REF" "$LIB_REPO" "$REPO_DIR" 2>/dev/null; then
        rm -rf "$REPO_DIR"
        git clone "$LIB_REPO" "$REPO_DIR"
        git -C "$REPO_DIR" checkout --detach "$LIB_REF"
    fi
else
    echo "Using cached clone at $REPO_DIR"
fi

# ---- step 3: prepare (cached via marker) -----------------------------------
if [ ! -f "$PREPARED_MARKER" ]; then
    echo "Preparing $LIB..."
    (cd "$REPO_DIR" && lib_prepare)
    touch "$PREPARED_MARKER"
else
    echo "Already prepared (cached)"
fi

# ---- step 4: bundle (cached) -----------------------------------------------
if [ ! -f "$BUNDLE" ]; then
    echo "Bundling $LIB_ENTRY with esbuild@$ESBUILD_VERSION..."
    (cd "$REPO_DIR" && "$ESBUILD" "$LIB_ENTRY" \
        --bundle \
        --format=iife \
        --platform="$LIB_ESBUILD_PLATFORM" \
        "${LIB_ESBUILD_EXTRA[@]}" \
        --outfile="$BUNDLE")
    echo "Bundle: $BUNDLE ($(wc -c < "$BUNDLE") bytes)"
else
    echo "Using cached bundle at $BUNDLE"
fi

# ---- step 5: prepend shims -------------------------------------------------
# node-buffer-shim.js (Buffer + TextEncoder/TextDecoder) is a shared shim
# alongside node-shim.js: Buffer is the highest-value host object (many
# libraries reference it at import time), so every bundle gets it.
SHIMS=("$SCRIPT_DIR/node-shim.js" "$SCRIPT_DIR/node-buffer-shim.js")
if [ -n "$LIB_SHIM" ]; then
    SHIMS+=("$SCRIPT_DIR/$LIB_SHIM")
fi
for shim in "${LIB_SHIMS[@]}"; do
    SHIMS+=("$SCRIPT_DIR/$shim")
done
cat "${SHIMS[@]}" "$BUNDLE" > "$FINAL"
echo "Final bundle: $FINAL ($(wc -c < "$FINAL") bytes)"

# ---- step 6: build jsse (unless node-only) ---------------------------------
if [ "$NODE_ONLY" -eq 0 ]; then
    echo "Building jsse (release)..."
    (cd "$PROJECT_DIR" && cargo build --release)
fi

# ---- run + evaluate helpers ------------------------------------------------
VERDICT=""; COUNT=""
evaluate() {   # <engine> <label>  → sets VERDICT/COUNT, returns 0 on PASS
    local engine="$1" label="$2"
    local out="$LIB_CACHE/out-$label.txt" rc=0 result
    # jsse needs --node to install the #229 __host_* syscall floor (byte I/O,
    # monotonic clock, process exit) that node-shim.js builds process/console/
    # util on. Node has no such flag, and the shim is guarded so it stays inert
    # there — that is what keeps `--node` a valid same-bundle reference oracle.
    local engine_args=()
    [ "$label" = "jsse" ] && engine_args=(--node)
    echo ""
    echo "========================================"
    echo "  Running $LIB test suite on $label"
    echo "========================================"
    if [ -n "$LIB_TIMEOUT" ]; then
        timeout "$LIB_TIMEOUT" env "${LIB_ENV[@]}" \
            "$engine" "${engine_args[@]}" "$FINAL" > "$out" 2>&1 || rc=$?
        [ "$rc" -eq 124 ] && echo "(timed out after ${LIB_TIMEOUT}s)" >> "$out"
    else
        env "${LIB_ENV[@]}" "$engine" "${engine_args[@]}" "$FINAL" > "$out" 2>&1 || rc=$?
    fi
    cat "$out"
    result="$(lib_verdict "$out" "$rc" || true)"
    VERDICT="${result%% *}"
    COUNT="${result##* }"
    echo "----------------------------------------"
    echo "  $label: $VERDICT (count=$COUNT, exit=$rc)"
    echo "========================================"
    [ "$VERDICT" = "PASS" ]
}

check_expected() {  # <label> <count> → 0 if LIB_EXPECT_COUNT unset or matches
    [ -z "$LIB_EXPECT_COUNT" ] && return 0
    if [ "$2" != "$LIB_EXPECT_COUNT" ]; then
        echo "MISMATCH: $1 count $2 != expected $LIB_EXPECT_COUNT" >&2
        return 1
    fi
}

# ---- node-only mode --------------------------------------------------------
if [ "$NODE_ONLY" -eq 1 ]; then
    if evaluate node node; then
        check_expected node "$COUNT" && { echo "OK: $LIB green on Node"; exit 0; }
    fi
    echo "FAILED: $LIB on Node" >&2
    exit 1
fi

# ---- jsse run --------------------------------------------------------------
JSSE_PASS=0
evaluate "$JSSE" jsse && JSSE_PASS=1
JSSE_COUNT="$COUNT"

FAIL=0
[ "$JSSE_PASS" -eq 1 ] || { echo "FAILED: $LIB on jsse" >&2; FAIL=1; }
check_expected jsse "$JSSE_COUNT" || FAIL=1

# ---- node cross-check ------------------------------------------------------
if [ "$CROSS_CHECK" -eq 1 ]; then
    if command -v node >/dev/null 2>&1; then
        NODE_PASS=0
        evaluate node node && NODE_PASS=1
        NODE_COUNT="$COUNT"
        [ "$NODE_PASS" -eq 1 ] || { echo "FAILED: $LIB on Node (reference)" >&2; FAIL=1; }
        check_expected node "$NODE_COUNT" || FAIL=1
        if [ "$JSSE_COUNT" != "$NODE_COUNT" ]; then
            echo "MISMATCH: jsse ran $JSSE_COUNT tests, Node ran $NODE_COUNT — jsse may have skipped tests" >&2
            FAIL=1
        fi
    else
        echo "WARNING: node not found — skipping cross-check (count unverified)" >&2
    fi
fi

echo ""
if [ "$FAIL" -eq 0 ]; then
    if [ "$CROSS_CHECK" -eq 1 ]; then
        echo "OK: $LIB green on jsse (cross-checked against Node: $JSSE_COUNT tests)"
    else
        echo "OK: $LIB green on jsse ($JSSE_COUNT tests)"
    fi
    exit 0
fi
echo "FAILED: $LIB" >&2
exit 1
