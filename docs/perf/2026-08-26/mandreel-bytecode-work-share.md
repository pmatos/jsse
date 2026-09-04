# Where mandreel's time goes under `--bytecode` (issue #526)

Generated 2026-08-26. Answers issue #526's open question — "where does the time
actually go inside compiled execution" — and corrects the metric both #526 and
#524 were built on.

## Headline

Two findings, one diagnostic and one causal.

**The metric misleads.** `mandreel`'s 96.5% compiled-**invocation** share is an
artifact of counting invocations. By **work**, the VM covers roughly 13% of
interpretive execution; two functions the compiler rejects hold 69.5% of the
tree-walker work that remains, and ~61% of all interpretive work in the run.

**The flat wall clock is arithmetic, not a mystery.** The VM saves ~22 ns per
opcode dispatched and costs ~350 ns per compiled-body entry, so a body breaks
even at ~16 opcodes. mandreel's compiled bodies average **15.87**. Its opcode
saving (~4.5 s) and its per-entry cost (~4.5 s) cancel to within the noise of a
205 s run — and the 96.5% invocation share is precisely *why* they cancel.

| metric | default | `--bytecode` |
| --- | --- | --- |
| VM opcodes dispatched | 0 | 202,105,685 |
| tree-walker work units | 1,548,788,858 | 1,359,253,247 |
| body dispatches — compiled | 0 | 12,737,766 (96.5%) |
| body dispatches — AST fallback | 13,195,918 | 458,152 (3.5%) |
| work per compiled body | — | **15.87 ops** |
| work per AST-fallback body | 117.37 units | **2,966.80 units** |

Enabling `--bytecode` moves 189.5 M of 1.55 B tree-walker work units (12.2%)
into the VM, where they become 202.1 M opcodes. A VM opcode and an `eval_expr`
entry are not equal-cost units, so 12–13% is directional, not exact — but two
orders of magnitude between 15.87 and 2,966.80 is not a units artifact.

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

**H1 — `Call` bridging dominates.** Refuted as stated, but it is the closest of
the three to the real answer. Only **384,216 of 13,195,918** `[[Call]]`s (2.9%)
originate inside compiled code, so bridging *out of* the VM cannot dominate
anything. What does matter is the cost of entering a compiled body at all —
paid on all 12.7 M entries regardless of who calls them, and quantified below.
The issue looked at the right seam from the wrong side.

**H2 — shared MOP paths.** Right in spirit, wrong as an explanation of the flat
result. Member/element opcodes are only 7.5 M of 202.1 M VM ops (3.7%):
`GetElement` 4,090,353, `SetElement` 3,384,860, `GetProp` 39,774; native calls
are 396,525 (3.0% of all calls). Shared MOP cost *is* what stops the VM's
percentage win from being as large as `arith`'s — see the `elem` row below — but
it does so by enlarging the denominator equally for both engines, not by making
compiled code slower. It is not what cancels mandreel.

**H3 — one-shot compile overhead.** Negligible. 182 compile attempts total
(79 successes, 103 bails) for the whole 3.5-minute run.

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

## What the VM is actually faster at, and what it costs

The work-share number says the VM barely gets to run. It does not say what
happens where it does. `benchmarks/scripts/bench_opmix.js` answers that: four
loops of equal iteration count, differing only in the kind of work each
iteration does.

| loop body, 1 M iterations | default ms | `--bytecode` ms | delta |
| --- | --- | --- | --- |
| `arith` — register arithmetic only | 4,110 [3,878-5,067] | 2,941 [2,801-3,553] | **-28.4%** |
| `elem` — typed-array element traffic | 6,360 [6,039-6,872] | 5,167 [4,903-5,762] | **-18.8%** |
| `called` — two leaf calls per iteration | 5,866 [5,768-6,555] | 5,231 [4,901-6,130] | **-10.8%** |
| `mixed` — mandreel's own mix of all three | 8,676 [8,180-10,222] | 7,487 [7,072-10,359] | **-13.7%** |

Medians of 7 pinned repeats, `[min-max]`. Every row exceeds the 5% stability
threshold `benchmark_protocol.py` uses, from interference on this shared host;
the medians held to within ~2 points across every partial view of the sweep
(n=1 through n=7). `arith` reproduces the ~-29% quoted in #524, which validates
the harness.

Percentages alone would suggest element traffic and calls both erode the VM's
advantage. **They do not erode it the same way, and reading only the
percentages gets the cause wrong.** Counting the opcodes each variant actually
dispatches (same driver, `--features perf-counters`, in
`opmix-opcounts.tsv`) and converting to absolute time per iteration:

| variant | opcodes / iter | compiled entries / iter | default µs/iter | `--bytecode` µs/iter | **saving** |
| --- | --- | --- | --- | --- | --- |
| `arith` | 49 | 0 | 4.11 | 2.94 | **1.169 µs** |
| `elem` | 55 | 0 | 6.36 | 5.17 | **1.193 µs** |
| `called` | 55 | 2 | 5.87 | 5.23 | **0.635 µs** |
| `mixed` | 71 | 1 | 8.68 | 7.49 | **1.189 µs** |

`elem` dispatches only 12% more opcodes than `arith`, so its smaller percentage
is not an opcode-count artifact — and its **absolute** saving is identical to
`arith`'s. Element traffic simply adds cost that *neither* engine reduces,
enlarging the denominator. The only variant whose absolute saving collapses is
the one that adds calls.

Fitting `saving = a·opcodes − b·entries` across the pairs gives

- **a ≈ 20-24 ns saved per opcode dispatched**, and
- **b ≈ 230-505 ns lost per compiled-body entry** (call-free rows pin `a`; the
  two call-bearing rows pin `b`, which is why its spread is wider).

So a compiled body only pays for itself above roughly **b/a ≈ 16 opcodes**.

### Why mandreel is exactly flat

mandreel's compiled bodies average **15.87 opcodes**. It sits on the break-even
point, and the two terms cancel:

| term | value |
| --- | --- |
| opcode saving | 202,105,685 ops × ~22 ns = **~4.5 s** |
| per-entry cost | 12,737,766 entries × ~350 ns = **~4.5 s** |
| net | **~0 s of 205.1 s** |

Across the plausible `a`/`b` range the model predicts between **-0.3% and
+0.4%**; the sweep measured +1.3% with overlapping ranges, i.e.
indistinguishable from zero. The flat wall clock is not a mystery to be
explained by any of H1/H2/H3 — it is arithmetic. And the 96.5% invocation share
is the *reason* it cancels: 12.7 M entries is a huge multiplier on `b`, while
15.87 opcodes each is a tiny multiplier on `a`.

### A code asymmetry the numbers do not convict

The tree-walker's computed member read has a numeric-index fast path
(`eval_member`, `src/interpreter/eval/access.rs:745`): for a Number key on a
typed array it calls `typed_array_get_index` directly, allocating nothing. The
VM's `Op::GetElement` has none — `member_get_computed`
(`src/interpreter/bytecode/vm.rs:110`) calls `to_property_key` unconditionally,
costing a `u32::to_string()` plus an `Arc<[u8]>` allocation before
`get_object_property` re-derives the index from the string it just built.

That asymmetry is real in the code. But it does **not** show up in these
numbers: if the VM paid a meaningful extra cost per element read, `elem`'s
absolute saving would be *below* `arith`'s, and it is marginally above. `elem`
does two such reads per iteration, so the penalty is bounded at well under
~25 ns per read here — below this measurement's resolution. Worth fixing for
its own sake (#538), but it is not what is holding mandreel back, and the
earlier draft of this report was wrong to sequence work behind it.

## What to do next

1. **Cut the per-entry cost of a compiled body.** This is the lever the
   measurement actually identifies, and it is the one that makes mandreel move
   without any eligibility change. `run_chunk_inner` allocates two fresh `Vec`s
   per entry (`Vec::with_capacity(max_stack)` and `max_refs`) and re-runs the
   `var_names` declaration prologue; pooling the operand and reference stacks on
   the `Interpreter` and indexing from a saved base would remove the allocations
   outright. Every 100 ns cut from `b` lowers the break-even body size by ~4.5
   opcodes, and mandreel's 12.7 M entries are all just below the current
   threshold. Filed as #539.
2. **Expand eligibility to labeled loops with labeled `break`/`continue`**
   (#524 item 2), which is what reaches the 69.5%. Projecting this in wall time
   rather than in work-unit share: mandreel converts AST work units to opcodes
   at 202.1 M / 189.5 M ≈ 1.07, so `sortMinDown` + `sortMaxDown`'s 944.9 M
   exclusive units become roughly 1.01 G opcodes. At ~22 ns/op that is **~22 s
   saved**, against a per-entry cost of 29,164 × 350 ns ≈ 0.01 s — their
   op-per-entry ratio is ~34,600, three orders of magnitude past break-even. So
   **≈ -10% on mandreel's 205 s**, and unlike step 1 it does not depend on `b`
   at all. (The 1.07 conversion ratio is measured on a different body mix, so
   treat the figure as one significant digit.)
3. **Then** `Op::GetElement`/`Op::SetElement`'s numeric-index fast path (#538) —
   a real asymmetry, but bounded above by these measurements at a cost too small
   to sequence other work behind.
4. **Not** call-site IC depth or the remaining `Call` opcode plumbing (#398),
   for mandreel's sake. 2.9% of `[[Call]]`s originate inside compiled code.

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
| `lexical declaration` | 1 |

The `lexical declaration` row is the top-level **script** body, whose compile
outcome only became visible after a later review pass (#537) — and it is the
harness's own `const __t0` timing marker on line 1, not mandreel code, so it
says nothing about the benchmark. Every other row is a function body.

Counts are per body, not per site, and one workload does not generalize — but
they should replace the guess for mandreel-shaped code, and the same counters
now exist to produce the equivalent table for typescript-octane and
OfflineAssembler.

## Method

- Counters: `cargo build --release --features perf-counters`, reported to stderr
  at exit. Re-collected after the #537 review fixes (strict tail-call counting,
  generator/eval attribution frames, all-exit-path reporting, and keeping
  non-function body executions out of the invocation split); every figure above
  is unchanged from the first collection. The top-level script body now shows up
  as `body_non_function_execs` 1 rather than inflating `body_dispatch_ast`, which
  is why that row reads 458,152 and not 458,153.
- Every count is deterministic: runs on five successive builds reported identical
  totals (`ast_work_units` 1,548,788,858 and `sortMinDown`'s 514,810,993 both
  reproduced exactly every time), so a shared host under variable load does not
  compromise them. Only two figures ever moved, both for known reasons:
  `compile_bail` 102 -> 103 once script-body compile outcomes began to be
  recorded at all, and `ast_units_per_ast_body` 2,966.82 -> 2,966.80 once that
  average's numerator was narrowed to function work only, excluding the script
  body's 7,787 units. (`body_dispatch_ast` also briefly read 458,153 under a
  defect since fixed.) No `BODY` row acquired a `#id` disambiguation suffix —
  every mandreel name is unique — so these figures survived the switch to
  identity-keyed attribution unchanged.
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
