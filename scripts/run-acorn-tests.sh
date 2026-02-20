#!/usr/bin/env bash
# Run acorn's test suite on jsse.
#
# Usage:
#   ./scripts/run-acorn-tests.sh [--clean] [--node]
#
# Clones acorn (cached in /tmp/acorn), bundles the test suite into a single
# IIFE file, prepends runtime shims, builds jsse, and runs the bundle.
# Use --clean to force a fresh acorn clone.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
ACORN_DIR="/tmp/acorn"
BUNDLE="/tmp/acorn-tests-bundle.js"
FINAL="/tmp/acorn-tests-final.js"
JSSE="$PROJECT_DIR/target/release/jsse"

ENGINE="$JSSE"
for arg in "$@"; do
    case "$arg" in
        --clean)
            echo "Cleaning cached acorn..."
            rm -rf "$ACORN_DIR" "$BUNDLE" "$FINAL"
            ;;
        --node)
            ENGINE="node"
            ;;
    esac
done

# Step 1: Clone and build acorn (cached)
if [ ! -d "$ACORN_DIR" ]; then
    echo "Cloning acorn..."
    git clone --depth 1 https://github.com/acornjs/acorn.git "$ACORN_DIR"
    echo "Installing acorn dependencies..."
    cd "$ACORN_DIR"
    # Remove test262 git dep (huge, causes integrity errors) — we only need rollup for build
    node -e "const p=require('./package.json'); delete p.devDependencies['test262']; delete p.devDependencies['test262-parser-runner']; require('fs').writeFileSync('package.json', JSON.stringify(p, null, 2)+'\n')"
    npm install
    echo "Building acorn..."
    npm run build
    cd "$PROJECT_DIR"
else
    echo "Using cached acorn at $ACORN_DIR"
fi

# Step 2: Bundle test suite with esbuild (cached)
if [ ! -f "$BUNDLE" ]; then
    echo "Bundling acorn test suite..."
    cd "$ACORN_DIR"
    npx esbuild test/run.js \
        --bundle \
        --format=iife \
        --platform=neutral \
        --main-fields=main,module \
        --outfile="$BUNDLE"
    cd "$PROJECT_DIR"
    BUNDLE_SIZE=$(wc -c < "$BUNDLE")
    echo "Bundle created: $BUNDLE ($BUNDLE_SIZE bytes)"
else
    echo "Using cached bundle at $BUNDLE"
fi

# Step 3: Prepend runtime shims
echo "Prepending runtime shims..."
cat "$SCRIPT_DIR/acorn-shim.js" "$BUNDLE" > "$FINAL"

FINAL_SIZE=$(wc -c < "$FINAL")
echo "Final bundle: $FINAL ($FINAL_SIZE bytes)"

# Step 4: Build jsse
if [ "$ENGINE" = "$JSSE" ]; then
    echo "Building jsse (release)..."
    cd "$PROJECT_DIR"
    cargo build --release
fi

# Step 5: Run on jsse
echo ""
echo "========================================"
echo "  Running acorn test suite on $(basename $ENGINE)"
echo "========================================"
echo ""

EXIT_CODE=0
"$ENGINE" "$FINAL" 2>&1 || EXIT_CODE=$?

echo ""
echo "========================================"
if [ $EXIT_CODE -eq 0 ]; then
    echo "  $(basename $ENGINE) exited successfully (code 0)"
else
    echo "  $(basename $ENGINE) exited with code $EXIT_CODE"
fi
echo "========================================"
