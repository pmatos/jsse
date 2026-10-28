# WeakRef targets and FinalizationRegistry cells are collectable

Status: accepted (#1317, epic #1214).

## Context

`WeakRef` kept its target in `primitive_value`, which `trace_object_fields`
visits, so every target was a strong root. A `FinalizationRegistry` already left
its targets and unregister tokens untraced, but nothing ever cleared them: the
callback never ran, and after a minor collection a cell could name a freed
(recyclable) id, so `unregister(newObject)` could match a stranger.
Node's AbortSignal and EventTarget tests assert that `gc()` empties a `WeakRef`
(`scripts/events-fixtures/`).

## Decision

- `ObjectKind::WeakRef(JsValue)` holds the target; the major trace skips it.
- **Minor collections keep weak slots strong** (`trace_weak_slots_strongly`:
  WeakRef targets, registry targets and tokens). Only a major collection
  decides a target is dead, so no weak slot can name a freed id and no weak
  edge needs a write barrier beyond the conservative `borrow_mut` one.
- **A major collection clears dead slots after marking** and before the sweep
  (`clear_dead_weak_slots`): a dead WeakRef target becomes `undefined`; a dead
  registry target becomes `undefined` (an *empty* cell, still `Some`, still
  unregistrable by its token, held value still traced); a dead token becomes
  `None`. A registry that gained an empty cell and has no job queued gets one
  (`FinalizationRegistryData::cleanup_queued`).
- The cleanup job is a `Microtask::Closure` rooted on the registry
  (`enqueue_finalization_cleanup`). `CleanupFinalizationRegistry` removes one
  empty cell, then calls the callback with its held value, so a throw leaves
  the rest for a re-queued job. Host choice: it is a job in the microtask queue,
  not a macrotask, so every loop (default, `--node`, agents) drains it.
- KeepDuringJob / ClearKeptObjects: `Interpreter::kept_alive` (a root set) gets
  a target from the `WeakRef` constructor and `deref`. `job_depth` counts
  running jobs (program run, promise job, timer or loop callback);
  `finish_job` clears the set when the outermost one ends, so a nested drain
  (top-level await, `evalScript`) cannot release targets early.
  `$262.clearKeptObjects()` is the test262 host hook.
- Symbols are never collected, so a symbol target is always live.

## Consequences

- A WeakRef target survives until the next major collection after its last
  reference and job end; `$262.gc()` is a major collection.
- Cleanup order across registries is queue order; a callback that throws
  is ignored by the drain, as for any microtask.
- Tests: `test262-extra/WeakRef-target-*.js`,
  `test262-extra/FinalizationRegistry-cleanup-callback-after-collection.js`,
  unit tests in `gc_weak_slot_tests.rs`, including the issue's timer repro (a
  throw inside a timer callback is swallowed, so it cannot be a `tests/*.js`).
