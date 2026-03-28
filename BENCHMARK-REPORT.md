# JSSE Performance Benchmark Report

**Date:** 2026-03-28

## Engine Versions

| Engine | Version | Git SHA | Binary |
|--------|---------|---------|--------|
| JSSE | main | `845ad427cc7e74fb2fbf1d74446d1c72417112e5` | `./target/release/jsse` |
| Node.js | v25.7.0 | (system) | `/usr/bin/node` |
| Boa | 1.0.0-dev | `f075094f9674f9919b4d2e85ca1bcba410bf34b0` | `/tmp/boa/target/release/boa` |

## Methodology

Three established JavaScript benchmark suites were used:

- **SunSpider 1.0.2** — 26 micro-benchmarks (3D, crypto, string ops, math, regex)
- **Kraken 1.1** — 14 benchmarks (audio, imaging, crypto, JSON)
- **Octane 2.0** — 14 benchmarks (constraint solvers, ray tracing, physics, compilers)

Each benchmark runs as a **single process invocation** with 3 repetitions (best-of-3 reported). This measures actual JavaScript execution time, not startup overhead. All engines built in release mode.

## Results

### SunSpider 1.0.2 (26 tests — all engines pass all)

| Test | JSSE | Node.js | Boa |
|------|------|---------|-----|
| 3d-cube | 697.6ms | 28.1ms | 66.8ms |
| 3d-morph | 549.9ms | 21.9ms | 82.3ms |
| 3d-raytrace | 959.5ms | 23.6ms | 75.6ms |
| access-binary-trees | 625.1ms | 19.8ms | 68.0ms |
| access-fannkuch | 1,764.9ms | 25.3ms | 168.2ms |
| access-nbody | 559.0ms | 23.0ms | 62.5ms |
| access-nsieve | 620.9ms | 23.7ms | 138.3ms |
| bitops-3bit-bits-in-byte | 773.3ms | 20.8ms | 45.4ms |
| bitops-bits-in-byte | 852.6ms | 19.8ms | 64.5ms |
| bitops-bitwise-and | 542.9ms | 21.0ms | 371.7ms |
| bitops-nsieve-bits | 694.9ms | 23.5ms | 93.2ms |
| controlflow-recursive | 1,081.6ms | 19.9ms | 44.1ms |
| crypto-aes | 597.6ms | 23.6ms | 88.3ms |
| crypto-md5 | 770.4ms | 22.6ms | 41.4ms |
| crypto-sha1 | 658.0ms | 20.6ms | 40.6ms |
| date-format-tofte | 545.5ms | 24.9ms | 106.5ms |
| date-format-xparb | 278.4ms | 22.7ms | 59.7ms |
| math-cordic | 1,345.2ms | 22.0ms | 82.0ms |
| math-partial-sums | 267.4ms | 23.6ms | 123.0ms |
| math-spectral-norm | 673.3ms | 19.6ms | 43.8ms |
| regexp-dna | 11,119.9ms | 24.8ms | 134.1ms |
| string-base64 | 484.5ms | 23.2ms | 62.1ms |
| string-fasta | 702.4ms | 23.0ms | 179.7ms |
| string-tagcloud | 11,472.7ms | 26.5ms | 171.1ms |
| string-unpack-code | 41,515.2ms | 29.5ms | 362.1ms |
| string-validate-input | 3,359.1ms | 28.3ms | 136.5ms |
| **Total** | **83,512ms** | **605ms** | **2,912ms** |

**JSSE/Node: 138x — JSSE/Boa: 29x**

### Kraken 1.1 (JSSE 12/14, Node 14/14, Boa 14/14)

| Test | JSSE | Node.js | Boa |
|------|------|---------|-----|
| ai-astar | 52,319.8ms | 68.5ms | 6,964.6ms |
| audio-beat-detection | FAIL | 53.8ms | 3,990.9ms |
| audio-dft | 35,007.1ms | 64.4ms | 3,057.1ms |
| audio-fft | 38,855.9ms | 50.1ms | 3,853.8ms |
| audio-oscillator | 24,941.0ms | 55.9ms | 4,804.9ms |
| imaging-darkroom | 60,115.5ms | 107.5ms | 3,527.4ms |
| imaging-desaturate | 58,438.9ms | 75.1ms | 6,523.8ms |
| imaging-gaussian-blur | TIMEOUT | 122.7ms | 36,070.8ms |
| json-parse-financial | 279.8ms | 36.5ms | 486.9ms |
| json-stringify-tinderbox | 139.7ms | 33.5ms | 226.7ms |
| stanford-crypto-aes | 14,142.4ms | 55.4ms | 1,471.5ms |
| stanford-crypto-ccm | 10,052.6ms | 53.9ms | 976.7ms |
| stanford-crypto-pbkdf2 | 24,770.1ms | 52.0ms | 2,266.3ms |
| stanford-crypto-sha256-iterative | 7,437.3ms | 33.1ms | 772.5ms |
| **Total (passing)** | **326,500ms** | **863ms** | **74,994ms** |

**JSSE/Node: 378x — JSSE/Boa: 4.4x (passing tests only)**

### Octane 2.0 (JSSE 10/14, Node 14/14, Boa 14/14)

| Test | JSSE | Node.js | Boa |
|------|------|---------|-----|
| richards | 8,503.0ms | 2,022.5ms | 2,021.8ms |
| deltablue | 13,128.6ms | 2,026.1ms | 2,042.6ms |
| crypto | TIMEOUT | 4,025.0ms | 17,854.0ms |
| raytrace | 47,411.4ms | 2,027.1ms | 7,081.7ms |
| earley-boyer | TIMEOUT | 4,035.1ms | 21,263.5ms |
| regexp | TIMEOUT | 2,054.3ms | 24,823.4ms |
| splay | 98,366.8ms | 2,093.6ms | 2,735.9ms |
| navier-stokes | 53,337.1ms | 2,026.5ms | 5,248.6ms |
| pdfjs | 35,489.5ms | 56.8ms | 4,333.9ms |
| code-load | 1,065.7ms | 1,035.7ms | 2,045.7ms |
| box2d | 51,772.9ms | 1,035.5ms | 3,344.7ms |
| gbemu | 73,945.3ms | 1,039.2ms | 7,264.4ms |
| zlib | 30.2ms | 29.1ms | 129.4ms |
| typescript | TIMEOUT | 965.5ms | 6,462.5ms |
| **Total (passing)** | **383,051ms** | **24,472ms** | **106,652ms** |

**JSSE/Node: 15.7x — JSSE/Boa: 3.6x (passing tests only)**

## Analysis

### Where JSSE stands

JSSE is a tree-walking interpreter with no JIT, no bytecode compilation, and no inline caches. In this context the results are unsurprising:

- **vs Node.js (V8 JIT):** 15x–378x slower. V8's optimizing compiler generates native code; a tree-walker cannot compete on hot loops.
- **vs Boa (also a Rust interpreter):** 3.6x–29x slower. Both are interpreters, so this gap is about implementation efficiency — and it's actionable.

### Bright spots

- **zlib** (Octane): JSSE 30ms vs Node 29ms vs Boa 129ms. Essentially tied with V8, faster than Boa.
- **code-load** (Octane): JSSE 1,066ms vs Node 1,036ms. Near-parity with V8.
- **json-parse-financial** (Kraken): JSSE 280ms vs Boa 487ms. JSSE is 1.7x faster than Boa.
- **json-stringify-tinderbox** (Kraken): JSSE 140ms vs Boa 227ms. JSSE is 1.6x faster than Boa.

### Worst offenders (biggest JSSE/Boa gaps)

| Benchmark | Suite | JSSE | Boa | JSSE/Boa |
|-----------|-------|------|-----|----------|
| string-unpack-code | SunSpider | 41,515ms | 362ms | **115x** |
| regexp-dna | SunSpider | 11,120ms | 134ms | **83x** |
| string-tagcloud | SunSpider | 11,473ms | 171ms | **67x** |
| splay | Octane | 98,367ms | 2,736ms | **36x** |
| string-validate-input | SunSpider | 3,359ms | 137ms | **25x** |
| controlflow-recursive | SunSpider | 1,082ms | 44ms | **25x** |
| raytrace | Octane | 47,411ms | 7,082ms | **6.7x** |
| imaging-darkroom | Kraken | 60,116ms | 3,527ms | **17x** |
| crypto-md5 | SunSpider | 770ms | 41ms | **19x** |
| math-cordic | SunSpider | 1,345ms | 82ms | **16x** |

### Optimization targets

The gaps cluster into a few categories:

1. **String operations** (115x, 67x, 25x gap vs Boa) — `string-unpack-code`, `string-tagcloud`, `string-validate-input`. Likely caused by inefficient string concatenation, `charCodeAt`, or `String.fromCharCode` in hot loops.

2. **Regular expressions** (83x gap) — `regexp-dna`. JSSE's regex engine is likely doing excessive backtracking or lacking basic optimizations.

3. **Deep recursion / function call overhead** (25x gap) — `controlflow-recursive`. Each function call likely has high overhead in environment setup, argument binding, and scope chain construction.

4. **Tight numeric loops** (16-19x gap) — `math-cordic`, `crypto-md5`. These are pure arithmetic in tight loops. Overhead per AST node walk adds up.

5. **Property access patterns** (36x gap) — `splay` does heavy object property access and tree manipulation. Prototype chain lookup and property resolution are likely slow.

### Suggested optimization path for the blog post

**Phase 1 — Quick wins (target: 2-5x improvement on worst cases):**
- Profile `string-unpack-code` and `string-tagcloud` to find the hot path. Likely string concatenation using `+=` in a loop — switching to a rope or buffer internally could help dramatically.
- Check if `charCodeAt` / `String.fromCharCode` are doing unnecessary allocations.

**Phase 2 — Function call overhead:**
- Profile `controlflow-recursive`. Reduce per-call overhead: pre-allocate environments, avoid cloning argument lists, use a faster scope chain representation.

**Phase 3 — Numeric loop overhead:**
- Profile `math-cordic`. Consider specializing numeric operations to avoid repeated type checks inside tight loops.

**Phase 4 — Regex:**
- Profile `regexp-dna`. Compare JSSE's regex implementation against Boa's. Consider using the `regex` crate as a backend.

Even a 2-3x improvement on the worst offenders would make a compelling blog post: "We identified the bottlenecks, applied targeted optimizations, and cut execution time by X% on industry-standard benchmarks."

## Reproducing

```bash
# Build JSSE
cargo build --release

# Run all suites
uv run scripts/run-benchmarks.py --suite sunspider kraken octane \
    --engines jsse node boa --timeout 120 --repetitions 3 \
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
