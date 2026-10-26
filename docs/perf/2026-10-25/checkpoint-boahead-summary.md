# Checkpoint 58606ad146 vs boa 0.22.0

- jsse: `58606ad14640042275c24b01897f27a1f0871338` (feat(host): add structuredClone, btoa and atob globals (#1207))
- jsse binary sha256: `f8d21592620df4f8`; boa sha256: `67440462902fc53a`
- host: buildbox3 ; started 2026-10-25T20:14:04Z ; JetStream c603c04 Use JetStream Next for main branch (#300) ; runner sha256 `d061f395bdad`

## Micro-benchmarks (median of 5 interleaved runs; ratio = jsse/boa, >1 = jsse slower)

| bench | jsse s | boa s | ratio | notes |
|---|---:|---:|---:|---|
| fib | 0.951 | 1.207 | 0.79x | jsse range 6% |
| closures | 1.307 | 1.898 | 0.69x |  |
| opmix | 6.918 | 10.864 | 0.64x |  |
| string | 0.108 | 0.463 | 0.23x | boa range 6% |
| array | 0.181 | 0.310 | 0.58x |  |
| object | 0.360 | 0.527 | 0.68x | jsse range 9% |
| loop | 0.616 | 1.538 | 0.40x |  |
| json | 0.107 | 0.308 | 0.35x |  |
| regex | 0.072 | 0.160 | 0.45x |  |

**Geomean slowdown vs boa: 0.50x** over 9 benchmarks (>1 = slower)
Startup (empty script, median of 11): jsse 15 ms, boa 29 ms

## JetStream 3 (--iterations 1, score ratio = boa/jsse, >1 = jsse slower)

- pass: jsse 48/48, boa 44/48; both pass: 44
- **geomean score ratio (boa/jsse) over both-pass: 0.53x** (jsse score geomean 4.49, boa 2.39)
- jsse wins 44, loses 0
- jsse fails: none; boa fails: Basic(error), OfflineAssembler(error), typescript-octane(error), validatorjs(error)

### Worst 10 gaps

| benchmark | jsse score | boa score | boa/jsse |
|---|---:|---:|---:|
| bigint-noble-secp256k1 | 4.85 | 4.32 | 0.89x |
| bigint-noble-ed25519 | 4.10 | 3.62 | 0.88x |
| richards | 2.54 | 2.14 | 0.84x |
| hash-map | 1.58 | 1.28 | 0.81x |
| splay | 5.43 | 4.32 | 0.79x |
| delta-blue | 3.56 | 2.76 | 0.77x |
| mandreel | 0.25 | 0.19 | 0.76x |
| proxy-vue * | 16.84 | 12.63 | 0.75x |
| bigint-bigdenary | 4.66 | 3.45 | 0.74x |
| proxy-mobx | 31.45 | 23.26 | 0.74x |

### All both-pass benchmarks

| benchmark | jsse | boa | boa/jsse |
|---|---:|---:|---:|
| bigint-noble-secp256k1 | 4.85 | 4.32 | 0.89x |
| bigint-noble-ed25519 | 4.10 | 3.62 | 0.88x |
| richards | 2.54 | 2.14 | 0.84x |
| hash-map | 1.58 | 1.28 | 0.81x |
| splay | 5.43 | 4.32 | 0.79x |
| delta-blue | 3.56 | 2.76 | 0.77x |
| mandreel | 0.25 | 0.19 | 0.76x |
| proxy-vue * | 16.84 | 12.63 | 0.75x |
| bigint-bigdenary | 4.66 | 3.45 | 0.74x |
| proxy-mobx | 31.45 | 23.26 | 0.74x |
| Box2D | 4.11 | 2.99 | 0.73x |
| stanford-crypto-pbkdf2 | 6.62 | 4.71 | 0.71x |
| stanford-crypto-sha256 * | 6.13 | 4.22 | 0.69x |
| bigint-noble-bls12-381 | 0.36 | 0.25 | 0.68x |
| gaussian-blur | 1.68 | 1.12 | 0.67x |
| raytrace-public-class-fields | 1.43 | 0.95 | 0.66x |
| json-parse-inspector | 4.81 | 3.16 | 0.66x |
| UniPoker | 5.40 | 3.50 | 0.65x |
| stanford-crypto-aes * | 4.01 | 2.56 | 0.64x |
| lazy-collections | 5.75 | 3.66 | 0.64x |
| navier-stokes | 8.64 | 5.44 | 0.63x |
| Babylon | 8.38 | 5.26 | 0.63x |
| cdjs | 0.90 | 0.54 | 0.60x |
| raytrace-private-class-fields | 1.32 | 0.79 | 0.60x |
| gbemu | 1.16 | 0.69 | 0.60x |
| raytrace | 1.79 | 1.06 | 0.60x |
| crypto | 5.57 | 3.28 | 0.59x |
| earley-boyer | 4.12 | 2.35 | 0.57x |
| bigint-paillier * | 2.45 | 1.38 | 0.56x |
| sync-fs | 3.55 | 2.00 | 0.56x |
| ai-astar | 5.71 | 3.08 | 0.54x |
| ML | 0.47 | 0.24 | 0.52x |
| Air | 13.37 | 6.61 | 0.49x |
| FlightPlanner | 13.93 | 6.85 | 0.49x |
| pdfjs | 3.46 | 1.61 | 0.47x |
| octane-code-load * | 106.38 | 46.30 | 0.44x |
| async-fs * | 4.55 | 1.97 | 0.43x |
| doxbee-async | 8.87 | 2.81 | 0.32x |
| doxbee-promise | 5.73 | 1.50 | 0.26x |
| regexp-octane | 5.07 | 1.22 | 0.24x |
| multi-inspector-code-load | 24.27 | 4.36 | 0.18x |
| js-tokens * | 3.87 | 0.69 | 0.18x |
| first-inspector-code-load | 24.39 | 4.36 | 0.18x |
| json-stringify-inspector | 36.76 | 4.12 | 0.11x |

(* = >5% repeat range on either engine)
