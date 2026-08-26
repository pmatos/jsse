# Where mandreel's time goes under `--bytecode` (issue #526)

Generated 2026-08-26. Answers issue #526's open question — "where does the time
actually go inside compiled execution" — and corrects the metric both #526 and
#524 were built on.

## Headline

`mandreel`'s 96.5% compiled-**invocation** share is an artifact of counting
invocations. By **work**, the VM covers roughly 13% of interpretive execution;
two functions the compiler rejects hold 69.5% of the tree-walker work that
remains, and ~61% of all interpretive work in the run.

| metric | default | `--bytecode` |
| --- | --- | --- |
| VM opcodes dispatched | 0 | 202,105,685 |
| tree-walker work units | 1,548,788,858 | 1,359,253,247 |
| body dispatches — compiled | 0 | 12,737,766 (96.5%) |
| body dispatches — AST fallback | 13,195,918 | 458,152 (3.5%) |
| work per compiled body | — | **15.87 ops** |
| work per AST-fallback body | 117.37 units | **2,966.82 units** |

Enabling `--bytecode` moves 189.5 M of 1.55 B tree-walker work units (12.2%)
into the VM, where they become 202.1 M opcodes. A VM opcode and an `eval_expr`
entry are not equal-cost units, so 12–13% is directional, not exact — but two
orders of magnitude between 15.87 and 2,966.82 is not a units artifact.

The invocation share and the work share disagree by a factor of ~7 because
compiled bodies are tiny and fallback bodies are enormous. Concretely, 12.35 M
of the 12.74 M compiled dispatches (96.9%) are this function:

```js
function uint(value) {
  if (value >= 0) return value;
  return 4294967296 + value;
}
```

"96.5% of invocations run compiled" is, to a first approximation, "a
two-statement helper called 12.3 million times runs compiled".

## Which bodies hold the work

Ranked by tree-walker work units spent *exclusive* of nested bodies, under
`--bytecode`. Names are demangled here for readability; the raw dumps carry the
Itanium-mangled symbols mandreel emits. `__runMandreelPhased` is the harness's
own copy of `runMandreel()` — its units are the benchmark's `heap32` copy loop,
which the real `runMandreel()` runs identically.

| body | exclusive units | share | invocations | bail reason |
| --- | --- | --- | --- | --- |
| `btAxisSweep3Internal<unsigned short>::sortMinDown` | 514,810,993 | 37.87% | 15,078 | `statement:Labeled` |
| `btAxisSweep3Internal<unsigned short>::sortMaxDown` | 430,101,850 | 31.64% | 14,086 | `statement:Labeled` |
| `__runMandreelPhased` (driver) | 55,682,108 | 4.10% | 1 | `call callee` |
| `my_mandreel_call_constructors` | 55,681,055 | 4.10% | 1 | `expression:New` |
| `initHeap` | 47,726,644 | 3.51% | 1 | `expression:New` |
| `btCollisionWorld::addCollisionObject` | 45,469,952 | 3.35% | 1,922 | `statement:Labeled` |
| `insertleaf` | 42,321,640 | 3.11% | 9,603 | `statement:Labeled` |
| `btAxisSweep3Internal<unsigned short>::setAabb` | 20,238,093 | 1.49% | 19,220 | `statement:Labeled` |
| `btDbvtBroadphase::setAabb` | 16,141,381 | 1.19% | 19,220 | `statement:Labeled` |
| `btDiscreteDynamicsWorld::solveConstraints` | 15,798,384 | 1.16% | 20 | `statement:Labeled` |

`sortMinDown` and `sortMaxDown` alone are **69.5%** of all tree-walker work
under `--bytecode` — ~61% of the run's total interpretive work once the VM's
202.1 M opcodes are counted too — spread over **29,164 invocations, 0.22%** of
the 13.2 M total. Invocation share cannot localize cost, and this is how far
apart the two metrics can be.

Both are the broadphase sweep-and-prune insertion sorts, and both bail for the
same reason — Mandreel emits C control flow as labeled `while(true)` loops with
labeled `break`/`continue`:

```js
_3: while(true){
  if(r13 ==0) { ... break _3; }
  ...
  continue _3;
}
```

## The three hypotheses in #526

All three are refuted or negligible.

**H1 — `Call` bridging dominates.** Refuted. Only **384,216 of 13,195,918**
`[[Call]]`s (2.9%) originate inside compiled code. The compiled bodies are
leaves; they barely call anything. Call bridging cannot dominate a cost it
touches 2.9% of.

**H2 — shared MOP paths.** Not the binding constraint at workload level.
Member/element opcodes are 7.5 M of 202.1 M VM ops (3.7%): `GetElement`
4,090,353, `SetElement` 3,384,860, `GetProp` 39,774. Native calls are 396,525
(3.0% of all calls). But see "The element-access trap" below — H2 is wrong about
*today's* cost and right about *tomorrow's*.

**H3 — one-shot compile overhead.** Negligible. 181 compile attempts total
(79 successes, 102 bails) for the whole 3.5-minute run.

**Not hypothesized, and worth ruling out: GC.** One major collection, 1 ms.
Irrelevant.

## Phase breakdown

Wall-clock, for the record — and to check the work-share story against the
thing the issue actually reports.

| phase | default ms | `--bytecode` ms | delta |
| --- | --- | --- | --- |
| top-level declarations | 9 [9-12] | 8 [8-9] | — |
| `setupMandreel()` | 87,373 [86,049-92,505] | 88,649 [82,920-95,400] | +1.5% |
| &nbsp;&nbsp;↳ `global_init()` | 18,258 [18,097-18,445] | 17,524 [17,391-18,187] | -4.0% |
| &nbsp;&nbsp;↳ `__init()` | 69,110 [67,946-74,054] | 71,121 [65,525-77,208] | +2.9% |
| `heap32` copy loop (3.98 M iters) | 9,432 [9,174-10,218] | 9,565 [9,226-10,472] | +1.4% |
| `__init()` (second call) | 68,358 [67,866-76,766] | 73,665 [68,132-74,234] | +7.8% |
| `render()` x 20 | 34,419 [33,741-38,780] | 35,734 [34,991-36,107] | +3.8% |
| **TOTAL** | **205,144 [203,819-206,128]** | **207,892 [202,605-208,977]** | **+1.3%** |

Medians of 3 pinned repeats, `[min-max]`; loadavg1 at each run's start ranged
12-60. Almost every row's ranges overlap between modes, so **no phase shows a
reproducible change in either direction** — including the two rows that look
like regressions. This reproduces #526's premise directly: 96.5% of invocations
compiled, and the workload does not move.

One-shot initialization is 76% of the run: `setupMandreel()` plus the second
`__init()` is 155.7 s of 205.1 s, consistent with the 2026-07-24 breakdown #526
cites from #54. `render()` x 20 is 17%, and is essentially 100% `__draw()`
(34,409 of 34,419 ms). The `heap32` copy loop is 4.6%.

And the flat result is exactly what the work share predicts. The VM covers ~13%
of interpretive work, and it is worth 10-28% on the work it covers (next
section), so the expected end-to-end gain is roughly 0.13 x 0.15 = **~2%** —
below this measurement's noise floor. The two numbers are not in tension; the
coverage is simply too small for the speedup to surface.

## What the VM is actually faster at

The work-share number says the VM barely gets to run. It does not say the VM
would help if it did. `benchmarks/scripts/bench_opmix.js` answers that: four
loops of equal iteration count, differing only in the kind of work each
iteration does.

| loop body, 1 M iterations | default ms | `--bytecode` ms | delta |
| --- | --- | --- | --- |
| `arith` — register arithmetic only | 4,110 [3,878-5,067] | 2,941 [2,801-3,553] | **-28.4%** |
| `elem` — typed-array element traffic | 6,360 [6,039-6,872] | 5,167 [4,903-5,762] | **-18.8%** |
| `called` — two leaf calls per iteration | 5,866 [5,768-6,555] | 5,231 [4,901-6,130] | **-10.8%** |
| `mixed` — mandreel's own mix of all three | 8,676 [8,180-10,222] | 7,487 [7,072-10,359] | **-13.7%** |

Medians of 7 pinned repeats, `[min-max]`. Every row exceeds the 5% stability
threshold `benchmark_protocol.py` uses, from occasional interference on this
shared host; the medians held to within ~2 points across every partial view of
the sweep (n=1 through n=7), so the ordering is robust even though individual
runs are not. `called` and `mixed` overlap heavily and should not be read as
separable from each other.

The VM wins on every shape, but the win shrinks sharply as soon as the loop does
anything besides arithmetic. Pure register arithmetic reproduces the ~-29%
figure quoted in #524, which validates the harness. Adding typed-array element
traffic to the identical loop structure gives up a third of that win; adding
leaf calls gives up nearly two-thirds; mandreel's own mix of both lands between
them. Cheaper opcode dispatch is only decisive when there is little else in the
iteration that both engines pay for equally.

The element-traffic decay has a specific, single-site cause. The tree-walker's
computed member read has a numeric-index fast path (`eval_member`,
`src/interpreter/eval/access.rs:745`): for a Number key on a typed array it
calls `typed_array_get_index` directly, allocating nothing. The VM's
`Op::GetElement` has none. `member_get_computed`
(`src/interpreter/bytecode/vm.rs:110`) calls `to_property_key` unconditionally,
which for an integer index does `(trunc as u32).to_string()` and then
`JsPropertyKey::from(String)` — **two heap allocations per element read** —
before a string-keyed `get_object_property` re-derives the index from the string
it just built. `Op::GetElement` also re-roots the whole operand stack per
access. So a compiled body's element reads take a slower path than the same
reads in the tree-walker, and only the VM's cheaper dispatch keeps the net
result positive.

This matters for the sequencing below, because `sortMinDown` and `sortMaxDown`
are exactly the element-heavy shape — `heap32[…]`/`heapU16[…]` on nearly every
line.

## What to do next

1. **Expand eligibility to labeled loops with labeled `break`/`continue`**
   (#524 item 2). This is the lever: it is what reaches mandreel's 69.5%.
   Nothing else in the measurement moves that work. Expect the `elem`/`mixed`
   win on the bodies it moves, not the `arith` one — `sortMinDown` and
   `sortMaxDown` are element-heavy with almost no calls, so -14% to -19% on
   them is the plausible band. Against the ~61% of total interpretive work they
   hold, that projects to roughly **-9% to -12% on mandreel's wall clock** —
   modest, but the first non-zero movement this benchmark has shown, and a
   falsifiable prediction rather than a hope.
2. **Then give `Op::GetElement`/`Op::SetElement` the numeric-index fast path the
   tree-walker already has.** It is a multiplier on step 1 rather than a
   precondition — measured, it is the difference between the `arith` and `elem`
   columns — and it is a single-site change with no eligibility risk.
3. **Do not** invest in call-site IC depth or the remaining `Call` opcode
   plumbing (#398) for mandreel's sake. 2.9% of `[[Call]]`s originate inside
   compiled code; there is nothing there to win.

For #524 specifically, the measured bail order **contradicts the order that
issue guessed**. Its stated "expected-biggest real-code blocker" is
`throw`/`try`; measured on mandreel, `statement:Try` causes **1** bail and
`statement:Labeled` causes **47**:

| bail reason | count |
| --- | --- |
| `statement:Labeled` | 47 |
| `call callee` | 39 |
| `expression:New` | 6 |
| `expression:This` | 4 |
| `expression:Typeof` | 2 |
| `constant pool overflow` | 1 |
| `expression:Object` | 1 |
| `statement:Break` | 1 |
| `statement:Try` | 1 |

Counts are per body, not per site, and one workload does not generalize — but
they should replace the guess for mandreel-shaped code, and the same counters
now exist to produce the equivalent table for typescript-octane and
OfflineAssembler.

## Method

- Counters: `cargo build --release --features perf-counters`, reported to stderr
  at exit. Every count is deterministic; two runs on two different builds
  reported identical totals (`ast_work_units` 1,548,788,858 and
  `sortMinDown` 514,810,993 both reproduced exactly), so a shared host under
  variable load does not compromise them.
- Timing: the pristine `HEAD` binary (`24dbeda`), built before any
  instrumentation existed, pinned with `taskset` to one CPU, run serially with
  nothing else on the box, medians of repeated runs with `/proc/loadavg`
  recorded per run. Counting builds and timing builds are never the same binary.
  This host cannot satisfy `benchmark_protocol.py`'s idle gate — it is shared,
  and loadavg1 sat between 12 and 66 throughout — so every timing table carries
  its `[min-max]` range and its stability flag rather than a bare median.
  Timings here are directional; the counter tables are not, and the conclusions
  rest on the counters.
- Driver: `scripts/gen-mandreel-phases.py` against JetStream Octane
  `mandreel.js`, reproducing `runMandreel()` inside a function so `var`
  bindings keep function scope.
- Op mix: `benchmarks/scripts/bench_opmix.js`.
- `perf` was unavailable: `kernel.perf_event_paranoid` is 3 on this host, which
  blocks unprivileged profiling, and `sudo` is not available. #526's suggested
  perf-profile route is therefore closed here; the counters replace it.
- Raw data in this directory: `counters-default.txt`,
  `counters-bytecode.txt` (counter dumps), `mandreel-phase-timings.tsv` and
  `opmix-timings.tsv` (every individual timing run). Every table above is
  derived from these.
- Host: AMD EPYC 7501, 2.0 GHz, 61 online CPUs, Linux 6.1. Absolute times are
  roughly 4x the Ryzen AI 9 HX 370 figures in #526; shares are what transfer.
