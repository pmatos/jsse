perf(bigint): VM BigInt operators still pay generic dispatch and ownership costs in fully compiled loops

The four-operator BigInt VM loop is 1.726x Boa and has 26,400,035 VM opcodes, zero AST work, zero collections and flat identifier interning. The 256-bit multiply/modulo VM variant is 1.398x; noble ed25519/secp256k1 are 1.402x/1.354x in the cold workload audit. Both engines use num-bigint 0.5.x, so this finding is not a different arbitrary-precision library comparison.

In the operator-loop profile, binary_slow is 12.95% self, bigint_binary 4.77%, eval_binary 4.26%, Arc<BigInt> drop_slow 4.77%, and the VM dispatcher 31.52%. [binary_arm](https://github.com/pmatos/jsse/blob/2ea6417535037ca58d78a433a72a2a0e2dfb4d77/src/interpreter/bytecode/vm.rs#L898) specializes Number pairs, while BigInt pairs pop operands and enter [binary_slow](https://github.com/pmatos/jsse/blob/2ea6417535037ca58d78a433a72a2a0e2dfb4d77/src/interpreter/bytecode/vm.rs#L109) → [eval_binary/bigint_binary](https://github.com/pmatos/jsse/blob/2ea6417535037ca58d78a433a72a2a0e2dfb4d77/src/interpreter/eval.rs#L2141). Results use [Arc-backed JsBigInt](https://github.com/pmatos/jsse/blob/2ea6417535037ca58d78a433a72a2a0e2dfb4d77/src/types.rs#L673); Boa uses Rc-backed BigInts. Atomic ownership overhead is a hypothesis, not an isolated causal measurement.

Potential direction: add native BigInt-pair VM arms and reduce redundant operand ownership/refcount transitions, retaining the existing exact BigInt operations. Measure owned/in-place opportunities where uniqueness is known. Keep the default-parameter and lexical-binding gaps separate: the default-form mulmod repro is 2.096x, while the simpler VM form is 1.398x.

## Measurements

JSSE HEAD `2ea6417535037ca58d78a433a72a2a0e2dfb4d77`; Boa HEAD `f0789a20d87ad52eef6460953929c2b3cdbf8716`; AMD Ryzen AI 9 HX 370, Rust 1.98.1, Node v26.8.2. Both engines use their default fat-LTO release profiles; Boa includes its default allocator/Intl features. Binary snapshots are fixed. Three alternating fresh CLI processes per successful case, sequential on CPU 0; 180 s / 8 GiB limits. Background desktop load was present: diagnostic measurements, with every sample and its range retained. Ratios above 1 mean JSSE is slower. V8 ratios use throughput; other rows use process wall time.

| Workload | Metric | JSSE median | Boa median | JSSE slowdown |
|---|---|---:|---:|---:|
| diagnostic-1010/bigint/bench_bigint_ops_vm_add | process wall seconds | 0.393799 | 0.228145 | 1.726x |
| diagnostic-1010/bigint/bench_bigint_mulmod256_vm | process wall seconds | 0.209128 | 0.149589 | 1.398x |
| diagnostic-1010/bigint/bench_bigint_shift_bitops | process wall seconds | 0.088346 | 0.078016 | 1.132x |
| jetstream/bigint-noble-ed25519 | process wall seconds | 0.516195 | 0.368121 | 1.402x |
| jetstream/bigint-noble-secp256k1 | process wall seconds | 0.446441 | 0.329564 | 1.355x |
| jetstream/bigint-noble-bls12-381 | process wall seconds | 5.567717 | 5.078894 | 1.096x |

JetStream rows are one complete workload iteration, including initialization/preparation and validation, per fresh process. They are not official JetStream scores or the default multi-iteration schedules. Timing uses the uninstrumented release binary; the separate release-nolto counter build is never used for speed ratios.

## Sampling and counter evidence

`diagnostic-1010/bigint/bench_bigint_ops_vm_add` self samples:

```text
31.52% jsse [.] jsse::interpreter::bytecode::vm::run_chunk_inner
12.95% jsse [.] jsse::interpreter::bytecode::vm::binary_slow
4.77% jsse [.] <alloc::sync::Arc<num_bigint::bigint::BigInt>>::drop_slow
4.77% jsse [.] <jsse::interpreter::Interpreter>::bigint_binary
4.26% jsse [.] <jsse::interpreter::Interpreter>::eval_binary
2.90% libc.so.6 [.] cfree
2.56% jsse [.] num_bigint::biguint::multiplication::scalar_mul
```

Counters: vm_ops=26400035, ast_work_units=0, gc_minor=0, gc_major=0, gc_ms=0, local_slot_misses=0, atom_intern_calls=94.

## Reproducer

The measured fixture is `diagnostic-1010/bigint/bench_bigint_ops_vm_add`. A standalone form of its core workload is:

```js
function main() {
  var a = BigInt("0x79be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798");
  var b = BigInt("0x483ada7726a3c4655da4fbfc0e1108a8fd17b448a68554199c47d08ffb10d4b8");
  var s = BigInt(2);
  var r = a, k = 0;
  for (var i = 0; i < 1200000; i++) {
    r = a + b;
    r = r * s;
    r = r & a;
    r = r >> s;
    k += 1;
  }
  return k + (r > a ? 1 : 0);
}
console.log("ops_vm_add", main());
```

Save as `/tmp/repro.js`, then run the same file with `taskset -c 0 /path/to/jsse /tmp/repro.js` and `taskset -c 0 /path/to/boa /tmp/repro.js`. Interleave at least three process runs and compare medians/checksums. For profiling use `perf record -F 499 -e cpu-clock:u --call-graph dwarf -- /path/to/jsse /tmp/repro.js`; keep instrumented counts separate from wall time.

## Acceptance / semantic constraints

Preserve mixed Number/BigInt errors, exact division/remainder/sign rules, negative exponents, shift semantics and exception ordering. Compare the pure VM operator loop, Number control, 256-bit arithmetic and real noble workloads. Never time the counter build.

Any implementation should retain the existing test262 pass baseline and run the full required repository gates. This audit changes no engine implementation.

## Related observations in this audit

These rows share the investigated path. Very small or overlapping differences remain observations; the sizeable reproducible cases and profiles above motivate this issue.

| Workload | Observed JSSE slowdown |
|---|---:|
| jetstream/bigint-noble-bls12-381 | 1.096x |
| jetstream/bigint-noble-ed25519 | 1.402x |
| jetstream/bigint-noble-secp256k1 | 1.355x |
| diagnostic-1010/bigint/bench_bigint_invert | 1.991x |
| diagnostic-1010/bigint/bench_bigint_literal | 1.493x |
| diagnostic-1010/bigint/bench_bigint_modpow | 1.101x |
| diagnostic-1010/bigint/bench_bigint_mulmod256_vm | 1.398x |
| diagnostic-1010/bigint/bench_bigint_ops_vm_add | 1.726x |
| diagnostic-1010/bigint/bench_bigint_shift_bitops | 1.132x |
| diagnostic-1010/bigint/bench_bigint_small | 1.682x |

Raw samples, immutable generated workloads, all 110 sampling profiles, counters and paired falsification probes are retained under `docs/perf/2026-10-14/boa-head-audit/` locally. The pinned source/reproducer and inline evidence above make the finding independently reproducible.
