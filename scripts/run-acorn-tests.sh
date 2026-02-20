#!/usr/bin/env bash
# Run acorn's test suite on jsse.
#
# Usage:
#   ./scripts/run-acorn-tests.sh [--clean]
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

# Handle --clean flag
if [[ "${1:-}" == "--clean" ]]; then
    echo "Cleaning cached acorn..."
    rm -rf "$ACORN_DIR" "$BUNDLE" "$FINAL"
fi

# Step 1: Clone and build acorn (cached)
if [ ! -d "$ACORN_DIR" ]; then
    echo "Cloning acorn..."
    git clone --depth 1 https://github.com/acornjs/acorn.git "$ACORN_DIR"
    echo "Installing acorn dependencies..."
    cd "$ACORN_DIR"
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
echo "Building jsse (release)..."
cd "$PROJECT_DIR"
cargo build --release

# Step 5: Run on jsse
echo ""
echo "========================================"
echo "  Running acorn test suite on jsse"
echo "========================================"
echo ""

EXIT_CODE=0
"$JSSE" "$FINAL" 2>&1 || EXIT_CODE=$?

echo ""
echo "========================================"
if [ $EXIT_CODE -eq 0 ]; then
    echo "  jsse exited successfully (code 0)"
else
    echo "  jsse exited with code $EXIT_CODE"
fi
echo "========================================"
