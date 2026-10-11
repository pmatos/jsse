# #899: fixed per-call work and `arguments` objects

Local before/after for the changes in this PR. **Not** the reference-box
protocol of `docs/perf/2026-10-10`: the host was shared (load average ~30 on 24
cores), there is no `valgrind`/`perf` here, and `boa` was not installed, so
there are no instruction counts and no boa ratios. Numbers are the median of 7
interleaved runs of `user+sys` CPU seconds (`resource.getrusage`) for the
pre-change release binary (`80aa515e`) vs this branch, release build.

| benchmark (`repros/calls/bench_calls_*.js`) | before | after | change |
|---|---:|---:|---:|
| fib (bytecode) | 1.59 s | 1.13 s | -29% |
| leaf (bytecode) | 1.47 s | 1.25 s | -15% |
| nested (bytecode) | 1.20 s | 0.88 s | -26% |
| method (bytecode) | 1.51 s | 1.22 s | -19% |
| closure_call (bytecode) | 1.23 s | 1.01 s | -18% |
| ctor (bytecode) | 1.40 s | 1.26 s | -11% |
| args (bytecode) | 1.42 s | 1.36 s | -4% |
| fib (`--no-bytecode`) | 1.43 s | 1.05 s | -27% |
| leaf (`--no-bytecode`) | 2.16 s | 1.81 s | -16% |
| args (`--no-bytecode`) | 2.21 s | 1.68 s | -24% |

## What `args` is bound by now

Isolating the pieces of `bench_calls_args` (300k iterations each, CPU s):

| shape | before | after |
|---|---:|---:|
| `f(a, b, c) { return a; }` | 0.26 | 0.18 |
| `function(){ return arguments.length }` called with 3 args | 0.94 | 0.76 |
| `withDefault(a, b = 2)` | 0.29 | 0.22 |
| `var o = [i, 1, 2]` (floor: one small object, no call) | 0.60 | 0.61 |
| `var o = {a:i, b:1, c:2, d:3, e:4, f:5}` | 0.70 | 0.68 |
| `var o = noopArgs(i, 1, 2)` (call + arguments object) | 0.74 | 0.61 |

An arguments object with three arguments (six own properties) now costs the
same as a six-property object literal, so what is left of `create_arguments_object`
is the engine's per-object allocation/GC cost (`JsObjectData` size, `class_name`
`String`, the spilled randomised `HashMap` above four properties), not property
definition. That is the territory of #909/#901, not this issue.

## Criteria status

- fib instr/call down >= 15%: met on CPU time (-29% / -27%); instruction counts
  not measurable here.
- <= 2 mallocs per call in a leaf loop: **not met**, not measured. The
  allocations this issue owned are gone (callee clone, deferred args Vec,
  `split_off`, per-chunk stacks, per-call `Vec<String>`/`HashSet`s). What
  remains is the `String` keys of `Environment.bindings` (`"this"` and each
  parameter name), which belong to the slot-keyed environments work (#898).
- `create_arguments_object` <= 3k instr: not measurable here (see above).
- `bench_calls_args` <= 2.5x boa: not measurable here (no boa); -4%/-24% locally.
- test262 pass list unchanged: full run 99,911/99,911, 0 regressions.
