# #1058: drop operand re-rooting, inline the GC safepoint poll

Two pieces of GC bookkeeping sat on the VM's hot path in runs that barely collect.

1. `root_operand_stack` copied every object id on the operand stack into `gc_temp_roots` before nine opcodes
   (`GetProp`, `GetPrivate`/`LoadMethodPrivate`, `SetPrivate`, `GetElement` slow path, `LoadMethod`,
   `LoadMethodElement`, `SetProp`, `SetElement` slow path, `ToPrimitiveKey`). Every object operand is already in
   `gc_bytecode_roots` (`push_value` roots it, `collect_gc_roots` appends that stack), so the copy is gone. Debug
   builds (`release-checked`, `cargo test`) assert before each former site that this frame's slice of
   `gc_bytecode_roots` holds one id per object operand, plus one for an object held by the Script completion
   accumulator, so a future raw `stack.push` of an object fails loudly.
2. `Op::Safepoint` and the back-edge `Op::Jump` called `Interpreter::gc_safepoint` unconditionally (about 25 Ir
   when nothing collects). `GcPacer` now keeps `poll`, the OR of `major_requested`, `minor_requested` and
   `stress_period != 0`, in step with every writer (`mark_major`/`mark_minor`/`sync_poll`), and
   `gc_safepoint_poll` tests it inline and calls a `#[cold]` out-of-line body only when set. Stress mode keeps
   `poll` true, so `stress_count` ticks at every safepoint exactly as before; under `cfg(test)` the poll is always
   taken so `safepoint_calls` still counts every safepoint. Only the VM's two sites use it.

The poll compiles to a load of one byte from the interpreter, a test and a branch to an out-of-line call (objdump of
the shipped binary); an earlier revision that tested the three fields separately saved 3.3% on `stmt` instead of 3.9%.

## Measured (buildbox3, shipped profile: fat LTO, cgu=1, nightly, `--locked`)

cachegrind `--cache-sim=no`, whole-process `I refs`. base = `origin/main` at `dfb74b0a`; new = the same tree plus
this change (new binary sha256 `57225de148ee...`, built from this PR's source apart from the added unit test; base
binary `7fc89b95e5da...`). JetStream drivers at `--iterations 1` (audit-workspace drivers); minimum of the runs taken.

| benchmark | base Ir | new Ir | delta | criterion |
|---|---:|---:|---:|---|
| richards | 15,157,947,628 | 14,391,114,112 | -5.06% | >= 4% |
| Box2D | 9,224,750,750 | 8,629,315,899 | -6.45% | >= 4% |
| delta-blue | 10,113,425,340 | 9,715,598,622 | -3.93% | >= 2.5% |
| splay | 11,259,570,002 | 10,922,751,712 | -2.99% | >= 2.5% |
| gaussian-blur | 32,726,747,056 | 31,941,605,745 | -2.40% | >= 2.5% (missed by 0.1) |
| hash-map | 24,469,462,605 | 23,750,661,223 | -2.94% | |
| Babylon | 3,756,299,261 | 3,586,148,635 | -4.53% | |
| bigint-bigdenary | 6,371,647,192 | 6,181,148,824 | -2.99% | |
| bigint-noble-bls12-381 | 77,815,643,978 | 76,248,855,590 | -2.01% | |
| proxy-vue | 1,657,193,339 | 1,628,320,224 | -1.74% | |
| Air | 2,866,522,069 | 2,831,072,344 | -1.24% | |
| bigint-noble-ed25519 | 8,248,882,537 | 8,135,453,075 | -1.38% | |
| bigint-noble-secp256k1 | 7,256,390,634 | 7,162,872,585 | -1.29% | |
| sync-fs | 13,020,548,009 | 12,902,850,461 | -0.90% | |
| async-fs | 11,113,039,409 | 11,045,774,934 | -0.61% | |
| lazy-collections | 8,900,672,897 | 8,872,700,531 | -0.31% | |
| fib | 8,475,654,087 | 8,394,879,823 | -0.95% | within 1% |
| proxy-mobx | median 878.1M (5 runs) | median 872.9M (5 runs) | -0.6% | noise |

proxy-mobx's total Ir is not run-to-run stable (base 857M-892M, new2 845M-890M over five runs; randomised
interner), so only medians mean anything; its single-run -1.51% / +2.09% readings are noise. No benchmark gains Ir.

Reproducer (the issue's `repro-vm-gc-bookkeeping.js`, Ir per iteration = (N=300000 minus N=100000) / 200000,
minimum of 3 runs per binary and N):

| mode | base | new | delta | criterion |
|---|---:|---:|---:|---|
| `obj` | 6,841.6 | 6,593.9 | -3.62% | >= 3% |
| `num` | 6,148.9 | 5,970.9 | -2.89% | must not regress |
| `stmt` | 4,458.0 | 4,286.0 | -3.86% | >= 4% (missed by 0.14) |

Collection counts (perf-counters build, `release-nolto`): `vm_ops`, `gc_minor`, `gc_major` identical for base and new
on richards (43,688,401 / 0 / 0), delta-blue (23,543,174 / 4 / 4), Box2D (36,079,584 / 3 / 3), splay
(26,130,958 / 141 / 7) and gaussian-blur (223,036,626 / 0 / 1).

## Correctness gates

`release-checked` build (debug assertions on, including the new operand-rooting check and the LIFO root-stack
asserts): `test262-extra/` 1410/1410 normal and `--no-bytecode`, and under `JSSE_GC_STRESS=7` in both modes; a 10%
test262 sample 11751/11751; `cargo test --profile release-checked --lib` 1107 tests. Under `JSSE_GC_STRESS=1` and `=2`
`test262-extra/using-for-head-dispose-at-loop-exit.js` fails identically on `origin/main` (stress-only, unrelated).
Seeded test262 samples under stress (N=7 5%, N=2 1%, N=1 0.3%, seed 11) give byte-identical failure sets for base and
new (15 / 14 / 11 pre-existing stress failures). Full test262 default and `--no-bytecode`: 99,911/99,911 each.
