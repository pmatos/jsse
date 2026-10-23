# `reentrancy` fuzz target: Proxy traps and generators calling back mid-operation

Status: accepted (#1177); builds on the `execute` target (ADR 2026-10-23-1400).

## Context

Random bytes almost never form a script in which a Proxy trap or a generator
`next()` mutates, closes or re-invokes the very object an in-flight builtin is
operating on. Those are the places where state held across user code goes
stale: lengths and indices cached across a `[[Get]]`, ids not rooted over a
collection, collections cleared under an iterator, buffers shrunk under a
typed array.

## Decision

- **Re-entrancy Plan.** `fuzz_reentrancy_bytes` (`src/fuzz_reentrancy/`)
  decodes the input into 1..=4 scenarios of six fields each: the operation
  re-entered (`Op`: property MOP ops, `for-in`, spread, destructuring,
  `Array.prototype.*`, `Map`/`Set` iteration and construction, Set methods,
  `JSON`, typed-array `fill`/`set`/`slice`/`sort`, `Promise.all`, `yield*`,
  ...), the host that calls back (`Host`: Proxy trap, generator, accessor,
  callback), the hook (which trap, which generator step, own vs inherited
  accessor, which callback), the action the callback performs (`Action`:
  grow, shrink, delete, redefine, freeze, call the same operation again,
  `next`/`return`/`throw` on the running generator, throw, return an
  invariant-violating trap result, ...), the invocation of the hook it fires
  on (0..=3) and the target shape (array, sparse, holes, `Map`, `Set`,
  object, resizable-buffer typed array). Decoding is total: reading past the
  end yields 0. There is no `arbitrary` dependency.
- **Rendering.** The plan renders to one self-contained JS program: a shared
  prelude, then per scenario an IIFE that sets up the target, installs the
  re-entry point, runs the operation under `try`/`catch`, and prints the trace
  log plus a state snapshot of the target from a timer (after microtasks).
  Corruption therefore shows up as a bytecode/tree-walker mismatch as well as
  a crash. Caps keep programs bounded: four action runs per scenario, nesting
  depth three, every number a small literal (native loops are not charged by
  the step budget, so a big length would be a harness-made timeout). Detach
  uses `ArrayBuffer.prototype.transfer`; the generated programs never use
  `$262` or `gc`, so every one is divergence-compared.
- **Oracle.** The rendered program goes through the same path as `execute`
  (`execute_and_compare`): panic, abort, sanitizer report, or Execution
  Divergence. No new oracle.
- **Seeds.** `scripts/gen-fuzz-reentrancy-corpus.py` writes one seed per
  (op, host) pairing plus a few multi-scenario seeds into
  `fuzz/corpus/reentrancy/`, reading variant order from the `ALL` tables in
  the Rust source. A unit test decodes every seed, requires every `Op` and
  `Host` to appear, and requires each rendered program to be comparable and
  divergence-free. Every seed holds a `0x00` byte so the file-mutating
  pre-commit hooks treat it as binary.
- **ClusterFuzzLite.** `.clusterfuzzlite/build.sh` builds `reentrancy` next
  to `parse` and `execute` with `max_len = 32` (a plan is at most 25 bytes).
  The existing PR (code-change) and batch jobs pick it up with no workflow
  logic change.
- Engine bugs the target finds are filed as their own issues, not fixed
  alongside the tooling.

## Consequences

- The 120 s PR fuzz budget is now split three ways; the daily 30 minute batch
  run is unchanged. `fuzz-seconds` is not raised here.
- As with `execute`, there is no earlier `main` build of the target, so a
  latent bug can fail the blocking PR check; file it and re-run.
- Some op/shape/host combinations are intentionally degenerate (a `Map`
  proxied through a trap that binds methods, a typed-array index accessor that
  cannot be defined); they trace the error and carry on.
- Follow-ups: running the plan under `JSSE_GC_STRESS`-style forced collection
  inside the fuzz process, `$262.gc` and `WeakRef` operations under a
  non-compare mode, a per-action coverage report, a dictionary, module inputs.
