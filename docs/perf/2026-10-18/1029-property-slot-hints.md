# #1029: per-site slot hints for named property access

After #1123 the largest property cost left is the key lookup itself: `PropertyMap::get` was 10.3% of richards' Ir (974M, about
82 Ir per lookup: a linear scan of up to 8 entries, or a hash probe for a spilled map) and 12.4% of Box2D's (777M, plus 5.5% in
the spilled-map hash probe).

## Change

Each `Chunk` carries one `Cell<u32>` per `names` entry (`Chunk::prop_hints`, built beside `global_hints`). `GetProp`, `LoadMethod`,
`SetProp` and `UpdateProp` pass it to `PropertyMap::get_hinted`/`get_mut_hinted` (`slot_hinted`): the remembered slot is accepted only
if that slot holds this very key allocation (`JsPropertyKey::same_allocation`; keys are unique in a map), otherwise the normal
lookup runs and a hit retrains the hint. A miss never touches the hint, so a method load keeps the slot of the prototype that holds
the method while its receiver's own miss still scans. The hint is verified on every use, so deletion, spilling, compaction and
polymorphic receivers cannot make it wrong; they only cost one failed compare.

Tried and dropped: a 64-bit per-map key filter (bit from length and end bytes) to answer own misses without the scan; it cost
more than it saved (richards +0.5%, Box2D +0.9% against hints alone).

No `unsafe`.

## Measured (buildbox3, shipped profile, cachegrind whole-process Ir; base = #1123 `73db045b`)

| benchmark | base Ir | new Ir | delta |
|---|---:|---:|---:|
| Box2D | 6,296,201,958 | 6,031,000,307 | -4.21% |
| Babylon | 2,561,503,367 | 2,479,290,063 | -3.21% |
| richards | 9,329,964,174 | 9,078,244,189 | -2.70% |
| proxy-vue | 1,237,386,651 | 1,204,743,260 | -2.64% |
| ML | 55,789,243,068 | 55,107,507,173 | -1.22% |
| Air | 1,920,690,852 | 1,897,473,517 | -1.21% |
| js-tokens | 4,978,053,619 | 4,921,172,765 | -1.14% |
| ai-astar | 10,806,958,936 | 10,686,515,733 | -1.11% |
| bigint-bigdenary | 4,494,763,941 | 4,453,669,833 | -0.91% |
| bigint-noble-bls12-381 | 60,631,746,007 | 60,124,842,803 | -0.84% |
| micro:array | 762,834,496 | 757,301,851 | -0.73% |
| bigint-noble-ed25519 | 7,184,351,835 | 7,141,036,534 | -0.60% |
| hash-map | 14,099,402,732 | 14,021,284,012 | -0.55% |
| micro:regex | 118,101,491 | 117,544,499 | -0.47% |
| sync-fs | 6,777,425,016 | 6,749,019,832 | -0.42% |
| doxbee-promise | 4,006,840,083 | 3,990,671,656 | -0.40% |
| micro:object | 1,483,060,391 | 1,477,112,457 | -0.40% |
| micro:promise_new | 5,874,038,788 | 5,855,104,964 | -0.32% |
| bigint-noble-secp256k1 | 6,305,891,379 | 6,286,838,658 | -0.30% |
| earley-boyer | 5,200,003,889 | 5,185,701,107 | -0.28% |
| micro:json | 314,819,034 | 313,966,474 | -0.27% |
| raytrace | 12,357,620,751 | 12,326,836,778 | -0.25% |
| stanford-crypto-sha256 | 4,530,724,051 | 4,522,657,325 | -0.18% |
| async-fs | 7,618,338,968 | 7,607,995,998 | -0.14% |
| Basic | 3,559,271,345 | 3,555,487,809 | -0.11% |
| json-parse-inspector | 9,168,778,959 | 9,166,807,603 | -0.02% |
| lazy-collections | 4,397,987,969 | 4,397,668,192 | -0.01% |
| micro:string | 342,986,755 | 342,982,000 | -0.00% |
| doxbee-async | 2,368,654,986 | 2,368,622,325 | -0.00% |
| micro:loop | 3,193,546,486 | 3,193,544,284 | -0.00% |
| proxy-mobx | 594,613,578 | 594,616,047 | +0.00% |
| gaussian-blur | 27,190,155,896 | 27,194,475,865 | +0.02% |
| micro:opmix | 53,075,258,421 | 53,087,270,844 | +0.02% |
| micro:closures | 5,849,206,803 | 5,851,657,261 | +0.04% |
| delta-blue | 6,442,128,686 | 6,447,553,729 | +0.08% |
| splay | 8,133,154,685 | 8,140,951,760 | +0.10% |
| micro:closure_create | 2,625,111,256 | 2,627,997,907 | +0.11% |
| micro:fib | 5,643,747,815 | 5,654,516,054 | +0.19% |
| crypto | 4,919,138,439 | 4,935,259,747 | +0.33% |
| navier-stokes | 3,852,497,853 | 3,912,350,182 | +1.55% |

Geometric mean over 40 workloads: -0.56%. Workloads without named-property hot spots move by code layout alone (navier-stokes reads
+1.5% in the shipped build and +0.07% in the symbolised build of the same source).

Per access (micro, minus the control loop, Ir): own store `o.a = i` 557 -> 538, `o.v3 = i` on a 6-property object 667 -> 538, own get
437 -> 423, prototype get 542 -> 529.
