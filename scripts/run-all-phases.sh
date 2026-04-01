#!/bin/bash
# Run benchmarks for all 4 optimization phases.
# Other engines (node, boa, engine262) run only in baseline since they don't change.
set -e

BENCH_DIR="$HOME/.bench"
NODE_BIN=$(source ~/.nvm/nvm.sh && nvm use node > /dev/null 2>&1 && which node)
BOA_BIN="$BENCH_DIR/boa/target/release/boa"
E262_BIN="$BENCH_DIR/engine262/lib/node/bin.mjs"

echo "=== Engine versions ==="
echo "Node: $($NODE_BIN --version)"
echo "Boa: $BOA_BIN"
echo "engine262: $E262_BIN"
echo ""

# Baseline: all engines
echo "=== BASELINE (all engines) === $(date)"
uv run scripts/run-benchmarks.py \
    --suite sunspider kraken octane \
    --engines jsse node boa engine262 \
    --jsse-binary "$BENCH_DIR/jsse-baseline" \
    --node-binary "$NODE_BIN" \
    --boa-binary "$BOA_BIN" \
    --engine262-binary "$E262_BIN" \
    --timeout 120 --repetitions 3 \
    --output benchmark-results-baseline.json

# Phase 1: jsse only
echo ""
echo "=== PHASE 1 (jsse only) === $(date)"
uv run scripts/run-benchmarks.py \
    --suite sunspider kraken octane \
    --engines jsse \
    --jsse-binary "$BENCH_DIR/jsse-phase1" \
    --timeout 120 --repetitions 3 \
    --output benchmark-results-phase1-jsse.json

# Phase 2: jsse only
echo ""
echo "=== PHASE 2 (jsse only) === $(date)"
uv run scripts/run-benchmarks.py \
    --suite sunspider kraken octane \
    --engines jsse \
    --jsse-binary "$BENCH_DIR/jsse-phase2" \
    --timeout 120 --repetitions 3 \
    --output benchmark-results-phase2-jsse.json

# Phase 3: jsse only
echo ""
echo "=== PHASE 3 (jsse only) === $(date)"
uv run scripts/run-benchmarks.py \
    --suite sunspider kraken octane \
    --engines jsse \
    --jsse-binary "$BENCH_DIR/jsse-phase3" \
    --timeout 120 --repetitions 3 \
    --output benchmark-results-phase3-jsse.json

echo ""
echo "=== ALL DONE === $(date)"
