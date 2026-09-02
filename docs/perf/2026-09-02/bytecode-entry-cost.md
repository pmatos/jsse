# Bytecode entry-cost investigation (#539)

## Outcome

Do not pool the bytecode operand and reference `Vec`s on the current engine.
The two allocations are real, but they account for only about **4–19 ns per
compiled Body entry** on this host, not the fitted **230–505 ns** from the
2026-08-26 op-mix sweep. Two whole-`Vec` pool implementations recovered no
measurable `called`-loop entry penalty and did not move Mandreel beyond noise;
the simpler non-inlined version instead regressed its Mandreel median.

The production and test changes used to evaluate the pool were therefore
reverted. This directory retains the negative result and raw data so a future
entry-path change does not repeat the same experiment.

No permanent per-entry clock was added to `perf-counters`: millions of
`Instant::now()` calls would distort the instrumented workload and violate the
feature's deterministic-count design.

## Entry-path probes

Temporary `perf-counters`-only spans timed 2,002,002 compiled entries of the
`called` variant (`N=1,000,000`, plus its 1,000-iteration warm-up). Each result
below subtracts an empty `Instant::now(); elapsed()` span measured in the same
entry. Values are the three-repeat range and median in nanoseconds per entry.

| entry work | ns/entry range | median | interpretation |
| --- | ---: | ---: | --- |
| bytecode-cache lookup / `Rc` clone | 0.9–2.6 | 2.3 | negligible |
| IC Body setup plus `this` clone | 9.2–23.6 | 22.4 | VM-shaped, but tree walking also enters the Body IC store |
| IC Body cleanup | 1.0–6.0 | 1.6 | negligible |
| `var_names` prologue | 31.0–70.2 | 63.3 | largest measured span, but symmetric with tree-walker declaration instantiation |
| both `Vec::with_capacity` calls | **4.0–18.6** | **14.0** | only unambiguously VM-added span, but far below the fitted cost |

These are directional upper bounds: the spans are smaller than the timer cost
itself (20–35 ns/entry), host load was 9.75–11.37, and the timer overhead is
subtracted rather than eliminated. The aggregate raw nanoseconds are in
`entry-cost-probes.tsv`.

A second diagnostic made one leaf compilable and an observably identical leaf
ineligible using an unreachable `throw` after its `return`. With both callers
compiled, the instrumented compiled leaf took 1,719 ms and the AST leaf 1,358
ms for two million calls, an upper-bound difference of about 180 ns/entry.
Only compiled entries paid the temporary clocks, so this cannot be read as the
true `b`; it confirms that allocation is only a small part of the aggregate
entry-path difference.

## Pool experiment

The evaluated design used independent operand/reference `Vec` free lists on
`Interpreter`, capped at 256 vectors and capacity 256. Each frame acquired
both vectors, reserved the chunk's static maxima, and released them through a
single post-dispatch cleanup point. Reference vectors were cleared before
pooling so `IdentifierRef::SpecificEnv` could not retain escaped environments.
Tests covered sequential reuse, nested compiled calls, stale environment
handles, and a hard call-depth throw spanning more frames than the pool cap.

The first version extracted the opcode loop and passed both vectors by mutable
reference. A second version removed double-reference calls and forced that
loop to inline, ruling out the helper call/reference indirection as the reason
the pool failed to improve the entry-cost signal. Neither version is in the
final diff.

## Op-mix result

All times are milliseconds for one million loop iterations. The final sweep
alternated detached current `main` (`1c9becc`) and the inlined pool candidate
on the maximum-frequency CPU set. Every range exceeded the repository's 5%
stability threshold, so medians are diagnostic.

| `--bytecode` variant | current `main` median [min–max] | pool median [min–max] |
| --- | ---: | ---: |
| `arith` | 722 [683–906] | 805 [671–825] |
| `elem` | 1,320 [1,257–1,619] | 1,401 [1,284–1,769] |
| `called` | 1,252 [1,202–1,562] | 1,334 [1,173–1,644] |
| `mixed` | 1,979 [1,861–2,421] | 2,097 [1,844–2,547] |

The useful within-binary comparison is `called − arith`, because `called`
adds two compiled leaf entries per iteration. It is **530 ms on current main
and 529 ms with the pool**: no measurable recovery across two million entries.
The absolute candidate medians moved together with `arith`, which is host/code
layout noise rather than an entry-specific win. All individual runs are in
`opmix-timings.tsv`.

An earlier pristine-current-main sweep (seven repeats, also retained) already
showed a much smaller call-specific penalty than the August EPYC data: median
absolute savings were 263 ms for `arith` and 219 ms for `called`, a gap of
roughly 22 ns per compiled entry rather than ~350 ns. Its wide ranges prevent a
precise replacement estimate, but agree with the allocation probes about the
order of magnitude.

## Mandreel result

JetStream checkout `c603c04db8505477867974a69789309ded2cc948`, Ryzen AI 9 HX
370 host, pinned to maximum-frequency CPUs `0-3,12-15`. The host could not pass
the idle gate, so it was disabled and every result is explicitly diagnostic.
The runner's default 80-iteration measurement exceeded the 600-second timeout;
the comparable matrix therefore used `--iterations 1 --repeats 3`, rebuilding
and executing the complete Mandreel script for every outer repeat.

| binary / mode | median ms [min–max] | repeat range | bytecode vs same binary |
| --- | ---: | ---: | ---: |
| current `main`, default | 33,082 [31,603–34,177] | 8.1% | — |
| current `main`, bytecode | 32,582 [29,672–34,957] | 17.8% | -1.5% |
| whole-`Vec` pool, default | 33,373 [31,373–33,629] | 7.2% | — |
| whole-`Vec` pool, bytecode | 36,689 [33,907–37,212] | 9.7% | **+9.9%** |
| inlined pool, default | 38,903 [38,331–69,119] | 80.3% | — |
| inlined pool, bytecode | 35,407 [29,385–66,841] | 127.5% | -9.0% |

Current main reproduces the issue's flat result: its mode ranges overlap almost
completely. The simple pool regressed rather than producing the predicted
gain. The inlined candidate ran during severe external interference (starting
load 33.39 for bytecode) and cannot support a claim in either direction. Taken
together with the stable deterministic fact that pool management did not
change opcode or entry counts, there is no evidence to ship either candidate.

## Correctness validation

After reverting both pool candidates, the repository gate passed unchanged:

- `./scripts/lint.sh` (format plus clippy with and without `perf-counters`)
- `cargo build --release`
- `cargo test --release`: 614 passed, 1 intentionally ignored; all integration
  tests and the smoke oracle passed
- full test262, default mode: 99,907 scenarios, 99,895 passed, 12 known
  failures, **0 regressions**, 275 new passes
- full test262, `--bytecode`: the identical 99,895 / 12 result, **0
  regressions**, 275 new passes

`test262-pass.txt` was not rewritten; the runner continued to read the baseline
from `origin/main`.

## Conclusion and next measurement

The August fit assigned `called`'s entire lost saving to compiled-Body entry.
The direct probes show that the recoverable allocation/deallocation component
is much smaller, while the issue already notes that `Op::Call` operand splitting
and root removal are paid only by the call-bearing rows. On current main,
`called` no longer exhibits the original ~350 ns/entry gap at all.

If entry cost becomes material again on a controlled host, measure AST-call to
compiled-body entry separately from `Op::Call` to compiled-body entry before
changing frame storage. A focused `Op::Call` experiment is the higher-value
next step; another whole-`Vec` pool is not.
