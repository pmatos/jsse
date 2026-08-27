# Bytecode work-share instrumentation

## Context

Issue #526 reports that `mandreel` under `--bytecode` dispatches 96.5% of its
function-body invocations through the VM yet shows no wall-clock change, and
asks where the time goes inside compiled execution. Issue #524 reports the
mirror-image problem — eligibility covering under 10% of called functions on
other real-world code — and names measured bail-*reason* counters as its
prerequisite before any eligibility expansion is designed.

Both issues rest on the same metric: the share of function-body *invocations*
that ran compiled. #526 notes the flaw in passing ("a compiled call can wrap a
whole loop") without following it through: the converse also holds, and is the
larger error. A single AST-fallback body can wrap a whole loop nest, so an
invocation count says nothing about how much interpretive work each side did.
Neither issue can be answered without a work-weighted metric, and no such
counter exists.

`perf` is not an option on the development host: `kernel.perf_event_paranoid`
is 3, which blocks unprivileged profiling entirely, and `sudo` is unavailable.
The measurement therefore has to come from inside the engine.

## Constraints

- The shipped binary must carry no counter writes. A per-opcode increment on a
  workload executing hundreds of millions of ops would inflate the very wall
  time the counters exist to explain.
- Counts must be deterministic, so that a measurement taken on a shared host
  under variable load is still exact and two runs are directly comparable.
- No ECMAScript behavior change, no `spec/`/`test262/` change, no test262
  baseline movement.
- The bail labels must name a construct precisely enough to drive #524's
  expansion order.

## Approaches considered

1. Temporary instrumentation, measured and reverted — the pattern the 2026-08-24
   audit used for #524's invocation counts. Rejected: both open issues need the
   same counters again, and the numbers cannot be re-derived or audited once the
   code is gone.
2. A runtime flag (`--perf-counters`) on the default binary. Rejected: the
   counter writes stay in the hot path whether or not the flag is set, so the
   timing binary and the counting binary would be the same binary — exactly the
   confound this work has to avoid.
3. A Cargo feature (`perf-counters`) compiling the counters in only when
   explicitly requested. Selected: the default build is unchanged, and a
   measurement run and a timing run are necessarily two different binaries.

## Design

A new `interpreter::perf_counters` module, compiled only under the feature,
owns a `PerfCounters` struct held on `Interpreter`. Every increment site is a
`#[cfg(feature = "perf-counters")]` block, so the default build sees none of
them.

Counters fall into four groups:

- **Work share.** Opcodes dispatched by `vm::run_chunk_inner`, with a
  per-opcode histogram; and tree-walker work units, counted as `exec_statement`
  and `eval_expr` entries. A VM op and an `eval_expr` entry are not equal-cost
  units, so the resulting share is directional rather than exact — but two
  orders of magnitude of difference are not a units artifact.
- **Dispatch outcome.** `dispatch_body` compiled versus AST invocations (the
  metric both issues already have), reported beside work per invocation so the
  two can never again be confused.
- **Per-body attribution.** For each AST-fallback body, the work units it spent
  *exclusive* of nested bodies. Generator/async, top-level script, and `eval`
  bodies reach statement execution through `exec_body`/`exec_eval_body` rather
  than `dispatch_body` and carry no function object, so each gets a frame under
  a synthetic label — without one, their work would be credited to whichever
  ordinary body sits below them on the stack, which is the single largest way
  this ranking can lie. Their *execution* counts stay out of
  `body_compiled`/`body_ast` — those two are the invocation split #524
  published, and `exec_body` fires once per generator state-machine step, so
  counting steps there understates the compiled share several-fold on
  generator-heavy code. They are reported separately as
  `body_non_function_execs`, explicitly a count of executions rather than
  invocations. An entry/exit stack records the unit counter at
  entry and charges each body's inclusive cost to its caller's child total, so
  no unit is counted twice and a body that merely calls other bodies is not
  credited with their work. Bodies are keyed by function name, interned per
  object id so ranking millions of dispatches allocates nothing.
- **Eligibility.** Compile successes, and bails counted both by reason and by
  the name of the body that bailed — the latter being what makes an expansion
  aimable, since a construct appearing in one body that holds most of the work
  matters more than a construct appearing in fifty leaf bodies.

`CompileError::Unsupported` already carries a `&'static str` reason, but its two
catch-all arms report only `"statement"` and `"expression"`. Those become
`statement_kind()`/`expression_kind()` calls naming the AST variant
(`statement:Labeled`, `expression:New`). Both matches are exhaustive, so a new
AST variant fails to compile until it is classified. This is a bail-path-only
change, unconditional because it costs nothing at runtime and makes `{:?}`
traces legible.

GC collections are the one place a timer is safe: collections number in the
thousands, not the hundreds of millions, so `Instant` around `gc_safepoint`'s
collection arms adds no measurable distortion and settles whether GC is a
factor at all.

The report renders one tab-separated line per metric to stderr at the end of
`execute_code`, with `PERF`/`BAIL`/`BODY`/`OP` prefixes so a run's phase output
on stdout stays separable from its counters on stderr.

## Harness

`scripts/gen-mandreel-phases.py` emits a per-phase-instrumented mandreel driver
from an unmodified JetStream `mandreel.js`. It reproduces `runMandreel()`
statement for statement inside a function — so every `var` keeps the
function-scope binding kind the real benchmark gives it — and replaces
`mandreelAppInit()` and `mandreelAppDraw()` with timed copies to attribute the
C entry points reached through them. The 5 MB driver is a regenerable build
artifact, never edited by hand.

`benchmarks/scripts/bench_opmix.js` isolates which op mix the VM actually makes
cheaper: four loops of equal iteration count differing only in whether each
iteration does pure register arithmetic, typed-array element traffic, calls to
a tiny leaf, or mandreel's own mix of all three.

## Validation

- Unit tests cover the report rendering and the exclusive-attribution
  arithmetic, including a nested body whose child work must not be credited to
  its parent.
- `scripts/lint.sh` and CI gain a clippy run with the feature enabled, so the
  gated code cannot rot behind the default build.
- Determinism is asserted empirically: two runs of the same workload, on
  different builds, must report identical counts.
- The default build's behavior is unchanged by construction; the full test262
  suite is run in both execution modes to confirm no baseline movement.

## Success criteria

A single instrumented run reports, for a real workload, the compiled/AST split
by work rather than by invocation, the bail reason for every ineligible body,
and a ranking of the bodies that hold the interpretive work — enough to decide
#526's open question and to order #524's expansion by measurement.
