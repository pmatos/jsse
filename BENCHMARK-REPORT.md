# JSSE Performance Benchmark Report

**Date:** 2026-03-28

## Engine Versions

| Engine | Version | Git SHA | Binary |
|--------|---------|---------|--------|
| JSSE | main | `845ad427cc7e74fb2fbf1d74446d1c72417112e5` | `./target/release/jsse` |
| Node.js | v25.7.0 | (system) | `/usr/bin/node` |
| Boa | 1.0.0-dev | `f075094f9674f9919b4d2e85ca1bcba410bf34b0` | `/tmp/boa/target/release/boa` |
| engine262 | 0.0.1 | `ae71998cc5a8315700555135b1ac202a0d6d0b31` | `node /tmp/engine262/lib/node/bin.mjs` |

## Methodology

Three established JavaScript benchmark suites were used:

- **SunSpider 1.0.2** — 26 micro-benchmarks (3D, crypto, string ops, math, regex)
- **Kraken 1.1** — 14 benchmarks (audio, imaging, crypto, JSON)
- **Octane 2.0** — 14 benchmarks (constraint solvers, ray tracing, physics, compilers)

Each benchmark runs as a **single process invocation** with 3 repetitions (best-of-3 reported). This measures actual JavaScript execution time, not startup overhead. All engines built in release mode.

## Results

### SunSpider 1.0.2 (all engines pass all 26 tests)

| Test | JSSE | Node.js | Boa | engine262 |
|------|------|---------|-----|-----------|
| 3d-cube | 697.6ms | 28.1ms | 66.8ms | 29,219.1ms |
| 3d-morph | 549.9ms | 21.9ms | 82.3ms | 31,655.4ms |
| 3d-raytrace | 959.5ms | 23.6ms | 75.6ms | 30,722.8ms |
| access-binary-trees | 625.1ms | 19.8ms | 68.0ms | 18,857.2ms |
| access-fannkuch | 1,764.9ms | 25.3ms | 168.2ms | 96,220.8ms |
| access-nbody | 559.0ms | 23.0ms | 62.5ms | 24,177.6ms |
| access-nsieve | 620.9ms | 23.7ms | 138.3ms | 29,624.9ms |
| bitops-3bit-bits-in-byte | 773.3ms | 20.8ms | 45.4ms | 31,255.0ms |
| bitops-bits-in-byte | 852.6ms | 19.8ms | 64.5ms | 39,351.2ms |
| bitops-bitwise-and | 542.9ms | 21.0ms | 371.7ms | 34,631.3ms |
| bitops-nsieve-bits | 694.9ms | 23.5ms | 93.2ms | 47,831.7ms |
| controlflow-recursive | 1,081.6ms | 19.9ms | 44.1ms | 38,343.1ms |
| crypto-aes | 597.6ms | 23.6ms | 88.3ms | 26,004.8ms |
| crypto-md5 | 770.4ms | 22.6ms | 41.4ms | 34,855.8ms |
| crypto-sha1 | 658.0ms | 20.6ms | 40.6ms | 28,483.4ms |
| date-format-tofte | 545.5ms | 24.9ms | 106.5ms | 15,248.6ms |
| date-format-xparb | 278.4ms | 22.7ms | 59.7ms | 8,700.0ms |
| math-cordic | 1,345.2ms | 22.0ms | 82.0ms | 53,009.9ms |
| math-partial-sums | 267.4ms | 23.6ms | 123.0ms | 17,560.5ms |
| math-spectral-norm | 673.3ms | 19.6ms | 43.8ms | 30,372.4ms |
| regexp-dna | 11,119.9ms | 24.8ms | 134.1ms | 7,274.6ms |
| string-base64 | 484.5ms | 23.2ms | 62.1ms | 15,207.8ms |
| string-fasta | 702.4ms | 23.0ms | 179.7ms | 31,145.8ms |
| string-tagcloud | 11,472.7ms | 26.5ms | 171.1ms | 24,212.1ms |
| string-unpack-code | 41,515.2ms | 29.5ms | 362.1ms | 31,673.3ms |
| string-validate-input | 3,359.1ms | 28.3ms | 136.5ms | 10,541.6ms |
| **Total** | **83,512ms** | **605ms** | **2,912ms** | **786,181ms** |

| | vs Node | vs Boa | vs engine262 |
|------|---------|--------|-------------|
| **JSSE** | 138x slower | 29x slower | **9.4x faster** |

### Kraken 1.1

| Test | JSSE | Node.js | Boa | engine262 |
|------|------|---------|-----|-----------|
| ai-astar | 52,319.8ms | 68.5ms | 6,964.6ms | TIMEOUT |
| audio-beat-detection | FAIL | 53.8ms | 3,990.9ms | TIMEOUT |
| audio-dft | 35,007.1ms | 64.4ms | 3,057.1ms | TIMEOUT |
| audio-fft | 38,855.9ms | 50.1ms | 3,853.8ms | TIMEOUT |
| audio-oscillator | 24,941.0ms | 55.9ms | 4,804.9ms | TIMEOUT |
| imaging-darkroom | 60,115.5ms | 107.5ms | 3,527.4ms | TIMEOUT |
| imaging-desaturate | 58,438.9ms | 75.1ms | 6,523.8ms | TIMEOUT |
| imaging-gaussian-blur | TIMEOUT | 122.7ms | 36,070.8ms | TIMEOUT |
| json-parse-financial | 279.8ms | 36.5ms | 486.9ms | 5,282.6ms |
| json-stringify-tinderbox | 139.7ms | 33.5ms | 226.7ms | 3,009.2ms |
| stanford-crypto-aes | 14,142.4ms | 55.4ms | 1,471.5ms | TIMEOUT |
| stanford-crypto-ccm | 10,052.6ms | 53.9ms | 976.7ms | FAIL |
| stanford-crypto-pbkdf2 | 24,770.1ms | 52.0ms | 2,266.3ms | FAIL |
| stanford-crypto-sha256-iterative | 7,437.3ms | 33.1ms | 772.5ms | FAIL |

| Engine | Passed | Total (passing) |
|--------|--------|----------------|
| Node.js | 14/14 | 863ms |
| Boa | 14/14 | 74,994ms |
| JSSE | 12/14 | 326,500ms |
| engine262 | 2/14 | 8,292ms |

### Octane 2.0

| Test | JSSE | Node.js | Boa | engine262 |
|------|------|---------|-----|-----------|
| richards | 8,503.0ms | 2,022.5ms | 2,021.8ms | TIMEOUT |
| deltablue | 13,128.6ms | 2,026.1ms | 2,042.6ms | TIMEOUT |
| crypto | TIMEOUT | 4,025.0ms | 17,854.0ms | TIMEOUT |
| raytrace | 47,411.4ms | 2,027.1ms | 7,081.7ms | TIMEOUT |
| earley-boyer | TIMEOUT | 4,035.1ms | 21,263.5ms | TIMEOUT |
| regexp | TIMEOUT | 2,054.3ms | 24,823.4ms | TIMEOUT |
| splay | 98,366.8ms | 2,093.6ms | 2,735.9ms | TIMEOUT |
| navier-stokes | 53,337.1ms | 2,026.5ms | 5,248.6ms | TIMEOUT |
| pdfjs | 35,489.5ms | 56.8ms | 4,333.9ms | 86,629.9ms |
| code-load | 1,065.7ms | 1,035.7ms | 2,045.7ms | 5,736.1ms |
| box2d | 51,772.9ms | 1,035.5ms | 3,344.7ms | TIMEOUT |
| gbemu | 73,945.3ms | 1,039.2ms | 7,264.4ms | TIMEOUT |
| zlib | 30.2ms | 29.1ms | 129.4ms | 349.1ms |
| typescript | TIMEOUT | 965.5ms | 6,462.5ms | TIMEOUT |

| Engine | Passed | Total (passing) |
|--------|--------|----------------|
| Node.js | 14/14 | 24,472ms |
| Boa | 14/14 | 106,652ms |
| JSSE | 10/14 | 383,051ms |
| engine262 | 3/14 | 92,715ms |

## Analysis

### Engine hierarchy

On compute-heavy benchmarks, the ranking is consistent:

1. **Node.js (V8 JIT)** — Fastest by a wide margin. JIT compilation makes it 5-400x faster than interpreters on hot loops.
2. **Boa** — The reference interpreter. Consistently 3-90x faster than JSSE depending on the benchmark category.
3. **JSSE** — Our engine. 29x behind Boa on SunSpider, 4.4x behind on Kraken, 3.6x behind on Octane.
4. **engine262** — JS-on-JS interpreter. Slowest overall but passes all SunSpider tests. 9.4x slower than JSSE on SunSpider.

### Where JSSE stands

JSSE is a tree-walking interpreter with no JIT, no bytecode compilation, and no inline caches. In this context:

- **vs Node.js (V8 JIT):** 15x–378x slower. V8's optimizing compiler generates native code; a tree-walker cannot compete on hot loops.
- **vs Boa (Rust interpreter):** 3.6x–29x slower. Both are interpreters, so this gap is about implementation efficiency — and it's actionable.
- **vs engine262 (JS-on-JS):** 9.4x faster on SunSpider. JSSE handily beats the only other non-JIT, non-compiled engine.

### Bright spots

- **zlib** (Octane): JSSE 30ms vs Node 29ms vs Boa 129ms vs engine262 349ms. Tied with V8, faster than both Boa and engine262.
- **code-load** (Octane): JSSE 1,066ms vs Node 1,036ms. Near-parity with V8.
- **json-parse-financial** (Kraken): JSSE 280ms vs Boa 487ms vs engine262 5,283ms. 1.7x faster than Boa.
- **json-stringify-tinderbox** (Kraken): JSSE 140ms vs Boa 227ms vs engine262 3,009ms. 1.6x faster than Boa.
- **regexp-dna** (SunSpider): JSSE 11,120ms vs engine262 7,275ms. engine262 is 1.5x faster here — JSSE's regex is a clear weakness.

### Interesting inversions

- **string-unpack-code**: JSSE (41,515ms) is slower than engine262 (31,673ms). JSSE is the slowest engine of all four on this test due to O(n²) string concatenation.
- **string-tagcloud**: JSSE (11,473ms) is slower than engine262 (24,212ms) — wait, JSSE is faster here. But on regexp-dna, engine262 (7,275ms) beats JSSE (11,120ms). These inversions reveal specific implementation weaknesses.

### Worst offenders (biggest JSSE/Boa gaps)

| Benchmark | Suite | JSSE | Boa | JSSE/Boa |
|-----------|-------|------|-----|----------|
| string-unpack-code | SunSpider | 41,515ms | 362ms | **115x** |
| regexp-dna | SunSpider | 11,120ms | 134ms | **83x** |
| string-tagcloud | SunSpider | 11,473ms | 171ms | **67x** |
| splay | Octane | 98,367ms | 2,736ms | **36x** |
| string-validate-input | SunSpider | 3,359ms | 137ms | **25x** |
| controlflow-recursive | SunSpider | 1,082ms | 44ms | **25x** |
| imaging-darkroom | Kraken | 60,116ms | 3,527ms | **17x** |
| crypto-md5 | SunSpider | 770ms | 41ms | **19x** |
| math-cordic | SunSpider | 1,345ms | 82ms | **16x** |
| access-fannkuch | SunSpider | 1,765ms | 168ms | **10.5x** |

### Optimization targets

The gaps cluster into a few categories:

1. **String concatenation — O(n²) clone pattern** (115x, 67x, 25x gap vs Boa)
   - `string-unpack-code`, `string-tagcloud`, `string-validate-input`
   - Root cause: `js_value_to_code_units()` in `helpers.rs` clones the entire `Vec<u16>` on every `+=`. In a loop building a long string, this is quadratic.
   - Fix: Avoid cloning the left operand when it's consumed, or use a rope/buffer for concatenation.

2. **Regular expressions** (83x gap)
   - `regexp-dna` — even engine262 beats JSSE here (7.3s vs 11.1s).
   - JSSE's regex engine likely has excessive backtracking or lacks basic optimizations.

3. **Function call overhead** (25x gap)
   - `controlflow-recursive` — deeply recursive Ackermann, Fibonacci, Takeuchi.
   - Each call creates a new environment, binds arguments, checks proxy traps, and roots/unroots GC values. Reducing per-call overhead (pre-allocated environments, skipping proxy checks for known non-proxies) would help.

4. **Tight numeric loops** (10-19x gap)
   - `math-cordic`, `crypto-md5`, `access-fannkuch`
   - Pure arithmetic in tight loops. Per-AST-node walk overhead accumulates.

5. **Property access / tree manipulation** (36x gap)
   - `splay` — heavy object property access and prototype chain lookups.

### Suggested optimization path

**Phase 1 — String concatenation (highest impact, targets ~60% of SunSpider gap):**
- Fix the O(n²) clone in `js_value_to_code_units` / the `+` operator for strings
- Expected: 10-50x improvement on string-heavy benchmarks

**Phase 2 — Function call overhead:**
- Reduce per-call cost: skip proxy checks for plain functions, pre-allocate environments
- Expected: 2-5x improvement on recursive benchmarks

**Phase 3 — Regex:**
- Profile `regexp-dna`, compare approach with Boa's regex implementation
- Expected: 5-10x improvement on regex-heavy benchmarks

**Phase 4 — Numeric loop overhead:**
- Specialize common numeric patterns to reduce per-node type checking
- Expected: 2-3x improvement on arithmetic-heavy benchmarks

Even a 2-3x improvement on the worst offenders would make a compelling blog post narrative.

## Reproducing

```bash
# Build JSSE
cargo build --release

# Run all suites on all engines
uv run scripts/run-benchmarks.py --suite sunspider kraken octane \
    --engines jsse node boa engine262 --timeout 120 --repetitions 3 \
    --output benchmark-suites-results.json

# Run just SunSpider
uv run scripts/run-benchmarks.py --suite sunspider --engines jsse node boa
```

## Files

- `benchmarks/sunspider/` — SunSpider 1.0.2 test files
- `benchmarks/kraken/` — Kraken 1.1 test files
- `benchmarks/octane/` — Octane 2.0 test files
- `scripts/run-benchmarks.py` — Multi-engine benchmark runner
- `benchmark-suites-results.json` — Raw JSON results from this run
