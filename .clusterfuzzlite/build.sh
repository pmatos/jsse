#!/bin/bash -eu
# Builds the ClusterFuzzLite targets. `differential` is intentionally absent:
# it needs node and a release jsse binary, so it runs in .github/workflows/fuzz.yml.

TARGETS=(parse execute)

cd "$SRC/jsse"

for target in "${TARGETS[@]}"; do
  cargo fuzz build -O --debug-assertions "$target"
  cp "fuzz/target/x86_64-unknown-linux-gnu/release/$target" "$OUT/"
  zip -jq "$OUT/${target}_seed_corpus.zip" "fuzz/corpus/$target"/*
done

# Seeds carry the test262 harness inline, so allow longer inputs than the 4 KiB default.
cat > "$OUT/execute.options" <<'OPTIONS'
[libfuzzer]
max_len = 16384
OPTIONS
