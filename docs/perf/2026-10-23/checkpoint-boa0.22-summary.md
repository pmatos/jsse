# Checkpoint 136b059826 vs boa 0.22.0

- jsse: `136b059826bf232d151ba344dadbc8338facced8` (refactor(builtins): centralize reduction traversal (#1169))
- jsse binary sha256: `29927ee8a0d83933`; boa sha256: `600ab67e207e12a8`
- host: buildbox3 ; started 2026-10-23T08:46:28Z ; JetStream c603c04 Use JetStream Next for main branch (#300) ; runner sha256 `d061f395bdad`

## Micro-benchmarks (median of 5 interleaved runs; ratio = jsse/boa, >1 = jsse slower)

| bench | jsse s | boa s | ratio | notes |
|---|---:|---:|---:|---|
| fib | 0.935 | 1.199 | 0.78x | boa range 7% |
| closures | 1.268 | 2.093 | 0.61x |  |
| opmix | 6.790 | 12.700 | 0.53x |  |
| string | 0.106 | 0.494 | 0.21x |  |
| array | 0.178 | 0.312 | 0.57x | jsse range 21%; boa range 8% |
| object | 0.359 | 0.545 | 0.66x | jsse range 8%; boa range 7% |
| loop | 0.580 | 1.848 | 0.31x |  |
| json | 0.108 | 0.306 | 0.35x |  |
| regex | 0.072 | 0.160 | 0.45x | jsse range 53% |

**Geomean slowdown vs boa: 0.46x** over 9 benchmarks (>1 = slower)
Startup (empty script, median of 11): jsse 17 ms, boa 30 ms

## JetStream 3 (--iterations 1, score ratio = boa/jsse, >1 = jsse slower)

- pass: jsse 48/48, boa 44/48; both pass: 44
- **geomean score ratio (boa/jsse) over both-pass: 0.55x** (jsse score geomean 4.54, boa 2.51)
- jsse wins 44, loses 0
- jsse fails: none; boa fails: Basic(error), OfflineAssembler(error), typescript-octane(error), validatorjs(error)

### Worst 10 gaps

| benchmark | jsse score | boa score | boa/jsse |
|---|---:|---:|---:|
| bigint-noble-secp256k1 | 4.83 | 4.39 | 0.91x |
| proxy-vue * | 16.84 | 14.62 | 0.87x |
| bigint-noble-ed25519 | 4.13 | 3.56 | 0.86x |
| richards | 2.57 | 2.08 | 0.81x |
| proxy-mobx | 32.26 | 25.64 | 0.79x |
| splay | 5.57 | 4.32 | 0.78x |
| lazy-collections | 5.74 | 4.42 | 0.77x |
| bigint-bigdenary | 4.60 | 3.43 | 0.75x |
| sync-fs | 3.50 | 2.61 | 0.74x |
| hash-map | 1.62 | 1.21 | 0.74x |

### All both-pass benchmarks

| benchmark | jsse | boa | boa/jsse |
|---|---:|---:|---:|
| bigint-noble-secp256k1 | 4.83 | 4.39 | 0.91x |
| proxy-vue * | 16.84 | 14.62 | 0.87x |
| bigint-noble-ed25519 | 4.13 | 3.56 | 0.86x |
| richards | 2.57 | 2.08 | 0.81x |
| proxy-mobx | 32.26 | 25.64 | 0.79x |
| splay | 5.57 | 4.32 | 0.78x |
| lazy-collections | 5.74 | 4.42 | 0.77x |
| bigint-bigdenary | 4.60 | 3.43 | 0.75x |
| sync-fs | 3.50 | 2.61 | 0.74x |
| hash-map | 1.62 | 1.21 | 0.74x |
| delta-blue | 3.67 | 2.70 | 0.74x |
| Box2D | 4.26 | 3.13 | 0.74x |
| mandreel | 0.25 | 0.18 | 0.73x |
| UniPoker | 5.42 | 3.96 | 0.73x |
| bigint-noble-bls12-381 | 0.36 | 0.25 | 0.70x |
| bigint-paillier | 2.49 | 1.73 | 0.69x |
| gaussian-blur | 1.65 | 1.14 | 0.69x |
| stanford-crypto-sha256 | 6.09 | 4.02 | 0.66x |
| json-parse-inspector | 4.78 | 3.15 | 0.66x |
| stanford-crypto-pbkdf2 | 6.61 | 4.34 | 0.66x |
| Babylon | 8.38 | 5.44 | 0.65x |
| cdjs | 0.91 | 0.59 | 0.65x |
| raytrace-public-class-fields | 1.44 | 0.92 | 0.64x |
| earley-boyer | 4.11 | 2.62 | 0.64x |
| stanford-crypto-aes | 4.04 | 2.51 | 0.62x |
| gbemu | 1.21 | 0.72 | 0.60x |
| FlightPlanner | 14.29 | 8.39 | 0.59x |
| navier-stokes | 8.93 | 5.24 | 0.59x |
| raytrace-private-class-fields | 1.33 | 0.77 | 0.58x |
| crypto | 5.59 | 3.16 | 0.56x |
| async-fs | 4.62 | 2.58 | 0.56x |
| raytrace | 1.83 | 1.02 | 0.56x |
| ai-astar | 5.93 | 3.24 | 0.55x |
| Air | 13.33 | 7.23 | 0.54x |
| ML | 0.47 | 0.25 | 0.52x |
| pdfjs | 3.57 | 1.76 | 0.49x |
| doxbee-async | 8.79 | 4.12 | 0.47x |
| octane-code-load * | 104.17 | 48.54 | 0.47x |
| doxbee-promise | 6.18 | 2.38 | 0.38x |
| regexp-octane | 5.05 | 1.25 | 0.25x |
| js-tokens | 3.80 | 0.69 | 0.18x |
| first-inspector-code-load | 24.51 | 4.18 | 0.17x |
| multi-inspector-code-load * | 24.15 | 3.55 | 0.15x |
| json-stringify-inspector | 38.76 | 4.01 | 0.10x |

(* = >5% repeat range on either engine)
