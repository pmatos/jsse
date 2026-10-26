# Checkpoint 58606ad146 vs boa 0.22.0

- jsse: `58606ad14640042275c24b01897f27a1f0871338` (feat(host): add structuredClone, btoa and atob globals (#1207))
- jsse binary sha256: `f8d21592620df4f8`; boa sha256: `600ab67e207e12a8`
- host: buildbox3 ; started 2026-10-25T19:48:08Z ; JetStream c603c04 Use JetStream Next for main branch (#300) ; runner sha256 `d061f395bdad`

## Micro-benchmarks (median of 5 interleaved runs; ratio = jsse/boa, >1 = jsse slower)

| bench | jsse s | boa s | ratio | notes |
|---|---:|---:|---:|---|
| fib | 0.941 | 1.199 | 0.78x |  |
| closures | 1.274 | 2.086 | 0.61x |  |
| opmix | 6.930 | 12.716 | 0.54x |  |
| string | 0.101 | 0.496 | 0.20x | jsse range 6%; boa range 9% |
| array | 0.179 | 0.308 | 0.58x | boa range 15% |
| object | 0.364 | 0.546 | 0.67x |  |
| loop | 0.576 | 1.848 | 0.31x |  |
| json | 0.108 | 0.313 | 0.35x | jsse range 7%; boa range 19% |
| regex | 0.067 | 0.138 | 0.48x | jsse range 60%; boa range 35% |

**Geomean slowdown vs boa: 0.47x** over 9 benchmarks (>1 = slower)
Startup (empty script, median of 11): jsse 16 ms, boa 31 ms

## JetStream 3 (--iterations 1, score ratio = boa/jsse, >1 = jsse slower)

- pass: jsse 48/48, boa 44/48; both pass: 44
- **geomean score ratio (boa/jsse) over both-pass: 0.56x** (jsse score geomean 4.48, boa 2.51)
- jsse wins 44, loses 0
- jsse fails: none; boa fails: Basic(error), OfflineAssembler(error), typescript-octane(error), validatorjs(error)

### Worst 10 gaps

| benchmark | jsse score | boa score | boa/jsse |
|---|---:|---:|---:|
| bigint-noble-secp256k1 | 4.85 | 4.42 | 0.91x |
| proxy-vue * | 16.67 | 14.58 | 0.87x |
| bigint-noble-ed25519 * | 4.15 | 3.56 | 0.86x |
| richards | 2.54 | 2.09 | 0.82x |
| proxy-mobx | 31.65 | 25.77 | 0.81x |
| splay | 5.36 | 4.18 | 0.78x |
| Box2D | 4.09 | 3.13 | 0.76x |
| lazy-collections | 5.72 | 4.37 | 0.76x |
| hash-map | 1.58 | 1.20 | 0.76x |
| mandreel | 0.24 | 0.18 | 0.76x |

### All both-pass benchmarks

| benchmark | jsse | boa | boa/jsse |
|---|---:|---:|---:|
| bigint-noble-secp256k1 | 4.85 | 4.42 | 0.91x |
| proxy-vue * | 16.67 | 14.58 | 0.87x |
| bigint-noble-ed25519 * | 4.15 | 3.56 | 0.86x |
| richards | 2.54 | 2.09 | 0.82x |
| proxy-mobx | 31.65 | 25.77 | 0.81x |
| splay | 5.36 | 4.18 | 0.78x |
| Box2D | 4.09 | 3.13 | 0.76x |
| lazy-collections | 5.72 | 4.37 | 0.76x |
| hash-map | 1.58 | 1.20 | 0.76x |
| mandreel | 0.24 | 0.18 | 0.76x |
| sync-fs | 3.48 | 2.61 | 0.75x |
| delta-blue | 3.64 | 2.72 | 0.75x |
| bigint-bigdenary | 4.63 | 3.44 | 0.74x |
| UniPoker | 5.39 | 3.95 | 0.73x |
| bigint-paillier | 2.43 | 1.74 | 0.71x |
| bigint-noble-bls12-381 | 0.36 | 0.25 | 0.70x |
| gaussian-blur | 1.67 | 1.15 | 0.69x |
| stanford-crypto-pbkdf2 | 6.51 | 4.35 | 0.67x |
| Babylon | 8.31 | 5.49 | 0.66x |
| stanford-crypto-sha256 | 6.11 | 4.03 | 0.66x |
| cdjs | 0.90 | 0.59 | 0.65x |
| raytrace-public-class-fields | 1.41 | 0.92 | 0.65x |
| earley-boyer | 4.10 | 2.61 | 0.64x |
| stanford-crypto-aes | 4.01 | 2.53 | 0.63x |
| gbemu | 1.17 | 0.73 | 0.62x |
| json-parse-inspector | 4.73 | 2.90 | 0.61x |
| navier-stokes | 8.61 | 5.24 | 0.61x |
| FlightPlanner | 13.97 | 8.43 | 0.60x |
| raytrace-private-class-fields | 1.31 | 0.77 | 0.59x |
| raytrace | 1.78 | 1.02 | 0.57x |
| async-fs | 4.52 | 2.57 | 0.57x |
| ai-astar | 5.69 | 3.21 | 0.56x |
| crypto | 5.62 | 3.14 | 0.56x |
| Air | 13.40 | 7.15 | 0.53x |
| ML | 0.47 | 0.25 | 0.53x |
| pdfjs | 3.55 | 1.77 | 0.50x |
| doxbee-async | 8.59 | 4.02 | 0.47x |
| octane-code-load * | 106.38 | 45.87 | 0.43x |
| doxbee-promise | 5.66 | 2.22 | 0.39x |
| regexp-octane | 5.05 | 1.25 | 0.25x |
| js-tokens * | 4.00 | 0.70 | 0.17x |
| first-inspector-code-load | 24.27 | 4.19 | 0.17x |
| multi-inspector-code-load | 24.39 | 4.16 | 0.17x |
| json-stringify-inspector | 36.76 | 3.93 | 0.11x |

(* = >5% repeat range on either engine)
