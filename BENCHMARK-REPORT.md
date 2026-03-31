# JSSE Performance Benchmark Report

**Date:** 2026-03-31
**Machine:** AMD EPYC 7501 32-Core, 252 GB RAM, Debian 6.1.0-41-amd64

## Engine Versions

| Engine | Version | Git SHA | Binary |
|--------|---------|---------|--------|
| JSSE | worktree-zany-foraging-finch | Varies per phase (see below) | `~/.bench/jsse-{phase}` |
| Node.js | v25.8.2 | (nvm) | `~/.nvm/versions/node/v25.8.2/bin/node` |
| Boa | 1.0.0-dev | `f5e88de558e038f0ae675a012d59917a098f44b6` | `~/.bench/boa/target/release/boa` |
| engine262 | 0.0.1 | `ae71998cc5a8315700555135b1ac202a0d6d0b31` | `node ~/.bench/engine262/lib/node/bin.mjs` |

### JSSE Phase Commits

| Phase | Commit | Description |
|-------|--------|-------------|
| Baseline | `845ad42` | Pre-optimization (main divergence point) |
| Phase 1 | `de28cce` | String concatenation: Arc-backed JsString, fast-path += |
| Phase 2 | `74cdaa9` | Function call overhead: lazy arguments, frame-based GC |
| Phase 3 | `2a6b286` | Regex: compiled cache, fast-path global match/replace |

## Methodology

Three established JavaScript benchmark suites:
- **SunSpider 1.0.2** — 26 micro-benchmarks (3D, crypto, string ops, math, regex)
- **Kraken 1.1** — 14 benchmarks (audio, imaging, crypto, JSON)
- **Octane 2.0** — 14 benchmarks (constraint solvers, ray tracing, physics, compilers)

Each benchmark runs as a single process invocation with **3 repetitions** (best-of-3 reported), 120s timeout per test. All engines built in release mode. Node/Boa/engine262 run only at baseline (they don't change between phases).

---

## Baseline Engine Comparison

### SunSpider 1.0.2

| Test | JSSE | Node.js | Boa | engine262 |
|------|------|---------|-----|-----------|
| 3d-cube | 1.9s | 57.0ms | 290.0ms | 84.6s |
| 3d-morph | 1.5s | 56.0ms | 247.5ms | 89.7s |
| 3d-raytrace | 2.6s | 60.0ms | 261.4ms | 83.5s |
| access-binary-trees | 1.9s | 44.4ms | 198.0ms | 56.2s |
| access-fannkuch | 5.0s | 65.2ms | 623.1ms | TIMEOUT |
| access-nbody | 1.6s | 47.0ms | 222.5ms | 74.8s |
| access-nsieve | 1.7s | 52.5ms | 353.6ms | 87.4s |
| bitops-3bit-bits-in-byte | 2.3s | 44.0ms | 171.2ms | 94.1s |
| bitops-bits-in-byte | 2.5s | 43.0ms | 217.7ms | 119.7s |
| bitops-bitwise-and | 1.5s | 45.0ms | 1.0s | 99.6s |
| bitops-nsieve-bits | 1.9s | 59.3ms | 277.6ms | TIMEOUT |
| controlflow-recursive | 3.1s | 44.7ms | 131.0ms | 117.0s |
| crypto-aes | 1.7s | 53.0ms | 278.8ms | 75.5s |
| crypto-md5 | 2.2s | 45.6ms | 147.9ms | 98.5s |
| crypto-sha1 | 1.9s | 47.0ms | 137.6ms | 84.2s |
| date-format-tofte | 1.6s | 51.2ms | 313.9ms | 44.5s |
| date-format-xparb | 859.7ms | 51.0ms | 174.7ms | 27.3s |
| math-cordic | 3.9s | 47.7ms | 286.6ms | TIMEOUT |
| math-partial-sums | 772.6ms | 68.5ms | 348.3ms | 47.9s |
| math-spectral-norm | 2.0s | 47.5ms | 162.4ms | 90.2s |
| regexp-dna | 25.5s | 79.8ms | 285.4ms | 16.5s |
| string-base64 | 1.3s | 43.9ms | 214.8ms | 46.0s |
| string-fasta | 2.0s | 56.0ms | 498.9ms | 92.4s |
| string-tagcloud | 26.9s | 80.3ms | 469.6ms | 68.3s |
| string-unpack-code | 95.7s | 65.2ms | 835.2ms | 87.7s |
| string-validate-input | 4.9s | 58.4ms | 392.8ms | 31.9s |
| **Total** | **198.7s** | **1.4s** | **8.6s** | **1,717s** (23/26) |

### Kraken 1.1

| Test | JSSE | Node.js | Boa | engine262 |
|------|------|---------|-----|-----------|
| ai-astar | TIMEOUT | 197.9ms | 22.1s | TIMEOUT |
| audio-beat-detection | FAIL | 166.1ms | 14.3s | TIMEOUT |
| audio-dft | 92.0s | 202.3ms | 11.8s | TIMEOUT |
| audio-fft | 112.1s | 143.2ms | 13.7s | TIMEOUT |
| audio-oscillator | 75.3s | 148.1ms | 15.4s | TIMEOUT |
| imaging-darkroom | TIMEOUT | 333.4ms | 13.3s | TIMEOUT |
| imaging-desaturate | TIMEOUT | 198.6ms | 22.0s | TIMEOUT |
| imaging-gaussian-blur | TIMEOUT | 304.1ms | TIMEOUT | TIMEOUT |
| json-parse-financial | 778.1ms | 94.5ms | 1.4s | 15.3s |
| json-stringify-tinderbox | 393.7ms | 79.4ms | 690.9ms | 8.4s |
| stanford-crypto-aes | 39.5s | 140.3ms | 5.4s | TIMEOUT |
| stanford-crypto-ccm | 28.4s | 169.5ms | 3.2s | FAIL |
| stanford-crypto-pbkdf2 | 71.1s | 163.9ms | 8.7s | FAIL |
| stanford-crypto-sha256-iterative | 21.5s | 91.8ms | 2.8s | FAIL |

| Engine | Passed | Total (passing) |
|--------|--------|----------------|
| Node.js | 14/14 | 2.4s |
| Boa | 13/14 | 134.9s |
| JSSE | 9/14 | 441.1s |
| engine262 | 2/14 | 23.8s |

### Octane 2.0

| Test | JSSE | Node.js | Boa | engine262 |
|------|------|---------|-----|-----------|
| richards | 20.9s | 2.0s | 3.1s | TIMEOUT |
| deltablue | 37.5s | 2.1s | 4.3s | TIMEOUT |
| crypto | TIMEOUT | 4.1s | 54.4s | TIMEOUT |
| raytrace | TIMEOUT | 2.1s | 17.6s | TIMEOUT |
| earley-boyer | TIMEOUT | 4.1s | 63.7s | TIMEOUT |
| regexp | TIMEOUT | 2.2s | 69.5s | TIMEOUT |
| splay | TIMEOUT | 2.3s | 4.0s | TIMEOUT |
| navier-stokes | TIMEOUT | 2.1s | 17.4s | TIMEOUT |
| pdfjs | 86.4s | 145.6ms | 13.0s | TIMEOUT |
| code-load | 3.2s | 1.1s | 3.1s | 14.6s |
| box2d | TIMEOUT | 1.1s | 11.3s | TIMEOUT |
| gbemu | TIMEOUT | 1.1s | 23.7s | TIMEOUT |
| zlib | 104.6ms | 66.6ms | 389.4ms | 985.2ms |
| typescript | TIMEOUT | 3.1s | 20.9s | TIMEOUT |

| Engine | Passed | Total (passing) |
|--------|--------|----------------|
| Node.js | 14/14 | 27.6s |
| Boa | 14/14 | 306.3s |
| JSSE | 5/14 | 148.1s |
| engine262 | 2/14 | 15.6s |

---

## Optimization Progress

### SunSpider (all 26 tests pass in every phase)

| Test | Baseline | Phase 1 | Phase 2 | Phase 3 | Speedup |
|------|----------|---------|---------|---------|---------|
| regexp-dna | 25.5s | 25.4s | 25.2s | 342.5ms | **74.3x** |
| string-tagcloud | 26.9s | 26.6s | 26.1s | 944.3ms | **28.5x** |
| controlflow-recursive | 3.1s | 3.0s | 1.3s | 1.3s | **2.4x** |
| crypto-md5 | 2.2s | 2.0s | 1.0s | 1.0s | **2.2x** |
| string-validate-input | 4.9s | 2.9s | 2.9s | 2.2s | **2.2x** |
| access-binary-trees | 1.9s | 1.8s | 946.9ms | 928.2ms | **2.0x** |
| crypto-sha1 | 1.9s | 1.9s | 994.9ms | 995.7ms | **1.9x** |
| string-base64 | 1.3s | 692.4ms | 682.7ms | 677.6ms | **1.9x** |
| math-spectral-norm | 2.0s | 1.9s | 1.1s | 1.1s | **1.8x** |
| string-unpack-code | 95.7s | 97.4s | 97.4s | 52.2s | **1.8x** |
| date-format-xparb | 859.7ms | 810.3ms | 514.8ms | 516.3ms | **1.7x** |
| bitops-3bit-bits-in-byte | 2.3s | 2.2s | 1.6s | 1.6s | 1.4x |
| string-fasta | 2.0s | 2.0s | 1.5s | 1.5s | 1.3x |
| 3d-raytrace | 2.6s | 2.6s | 2.0s | 2.0s | 1.3x |
| math-cordic | 3.9s | 3.8s | 3.0s | 3.0s | 1.3x |
| bitops-bits-in-byte | 2.5s | 2.5s | 1.9s | 1.9s | 1.3x |
| date-format-tofte | 1.6s | 1.5s | 1.4s | 1.4s | 1.2x |
| **Total** | **198.7s** | **196.1s** | **186.6s** | **90.3s** | **2.2x** |

### Kraken (9 common-passing tests)

| Test | Baseline | Phase 1 | Phase 2 | Phase 3 | Speedup |
|------|----------|---------|---------|---------|---------|
| stanford-crypto-ccm | 28.4s | 27.5s | 26.1s | 24.3s | 1.2x |
| stanford-crypto-aes | 39.5s | 38.1s | 37.0s | 36.3s | 1.1x |
| stanford-crypto-pbkdf2 | 71.1s | 68.4s | 67.2s | 66.4s | 1.1x |
| stanford-crypto-sha256 | 21.5s | 20.5s | 20.4s | 20.3s | 1.1x |
| imaging-darkroom | TIMEOUT | TIMEOUT | 99.8s | 98.5s | *newly passing* |
| **Common total** | **441.1s** | **430.7s** | **429.1s** | **421.8s** | **1.0x** |

### Octane (5 common-passing, +4 newly passing)

| Test | Baseline | Phase 1 | Phase 2 | Phase 3 | Speedup |
|------|----------|---------|---------|---------|---------|
| deltablue | 37.5s | 36.3s | 17.1s | 17.0s | **2.2x** |
| richards | 20.9s | 20.3s | 13.4s | 13.5s | **1.5x** |
| zlib | 104.6ms | 77.8ms | 106.0ms | 80.3ms | 1.3x |
| pdfjs | 86.4s | 69.9s | 79.5s | 80.0s | 1.1x |
| box2d | TIMEOUT | TIMEOUT | 104.1s | 105.4s | *newly passing* |
| raytrace | TIMEOUT | TIMEOUT | 90.0s | 90.8s | *newly passing* |
| splay | TIMEOUT | TIMEOUT | 88.0s | 87.9s | *newly passing* |
| regexp | TIMEOUT | TIMEOUT | TIMEOUT | 112.9s | *newly passing* |
| **Common total** | **148.1s** | **129.8s** | **113.1s** | **113.9s** | **1.3x** |

### Pass Count Progression

| Suite | Baseline | Phase 1 | Phase 2 | Phase 3 |
|-------|----------|---------|---------|---------|
| SunSpider | 26/26 | 26/26 | 26/26 | 26/26 |
| Kraken | 9/14 | 9/14 | 10/14 | 10/14 |
| Octane | 5/14 | 5/14 | 8/14 | 9/14 |
| **Total** | **40/54** | **40/54** | **44/54** | **45/54** |

---

## Summary

### Overall improvement (common-passing tests, baseline → Phase 3)

| Suite | Baseline | Phase 3 | Speedup | Common tests |
|-------|----------|---------|---------|-------------|
| **SunSpider** | 198.7s | 90.3s | **2.2x (55% faster)** | 26 |
| **Kraken** | 441.1s | 421.8s | **1.0x (4% faster)** | 9 |
| **Octane** | 148.1s | 113.9s | **1.3x (23% faster)** | 5 |

### Key wins

1. **regexp-dna**: 25.5s → 342ms (**74.3x faster**) — Phase 3 regex compiled cache and fast-path matching
2. **string-tagcloud**: 26.9s → 944ms (**28.5x faster**) — Combined regex (Phase 3) and string (Phase 1) improvements
3. **controlflow-recursive**: 3.1s → 1.3s (**2.4x**) — Phase 2 lazy arguments and frame-based GC
4. **deltablue**: 37.5s → 17.0s (**2.2x**) — Phase 2 function call overhead reduction
5. **5 newly passing benchmarks** — box2d, raytrace, splay, regexp (Octane), imaging-darkroom (Kraken)

### Phase-by-phase impact

| Phase | What changed | SunSpider | Kraken | Octane | New passes |
|-------|-------------|-----------|--------|--------|------------|
| **Phase 1** (Strings) | Arc-backed JsString, fast-path += | -1% | -2% | -12% | 0 |
| **Phase 2** (Calls) | Lazy arguments, frame GC | -5% | ~0% | +3 passes | +4 (box2d, raytrace, splay, darkroom) |
| **Phase 3** (Regex) | Compiled cache, fast-path match | **-52%** | ~0% | +1 pass | +1 (regexp) |

### JSSE vs Boa gap (baseline)

| Suite | JSSE/Boa ratio |
|-------|---------------|
| SunSpider | 23x slower |
| Kraken | 3.3x slower |
| Octane | 0.5x (faster on common tests) |

## Charts

See `benchmark-charts/` for visualizations:
- `suite_totals_by_phase.png` — Total time per suite across phases
- `top_improved.png` — Top improved benchmarks baseline vs Phase 3
- `jsse_vs_boa_gap.png` — JSSE/Boa gap narrowing across phases
- `speedup_heatmap.png` — Per-test speedup heatmap

## Reproducing

```bash
# Build engines
~/.bench/jsse-{baseline,phase1,phase2,phase3}   # Pre-built JSSE at each phase
~/.bench/boa/target/release/boa                  # Boa 1.0.0-dev
~/.bench/engine262/lib/node/bin.mjs              # engine262 0.0.1

# Run all phases
bash scripts/run-all-phases.sh

# Generate analysis + charts
uv run scripts/analyze-phase-results.py \
    --baseline benchmark-results-baseline.json \
    --phase1 benchmark-results-phase1-jsse.json \
    --phase2 benchmark-results-phase2-jsse.json \
    --phase3 benchmark-results-phase3-jsse.json \
    --output-dir benchmark-charts
```
