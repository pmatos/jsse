# Intrinsic-promise reactions without resolving-function objects (#915)

## Change

Every internal promise used to build a resolve/reject pair of full function
objects (two `create_function`, two `pin_native_root`, a push to
`PromiseData.resolving_functions`) even for `%Promise%`, where they are never
observable. They are now only built when something can observe them.

- `PromiseReaction` carries a `ReactionTarget`: `None`, `Capability` (a
  non-intrinsic constructor's resolve/reject, called as functions) or
  `Internal(promise_id)`, which the reaction job settles directly through
  `resolve_promise_internal` (the Promise Resolve Functions steps: self-check,
  observable `Get(then)`, thenable job, else fulfil) or `reject_promise`.
- `then`/`catch`/`finally` take the `Internal` target whenever
  `SpeciesConstructor` yields `%Promise%`. `Promise.resolve`/`PromiseResolve` and
  `Promise.reject` skip the capability for `%Promise%`.
- An async function's result promise is `PromiseResolvers::Internal`; the
  engine-owned `[[AlreadyResolved]]` flag is `PromiseData.already_resolved`.
  Module and dynamic-import machinery keeps `PromiseResolvers::Functions`.
- `PromiseResolveThenableJob` for a native promise whose `then` is the original
  runs `SpeciesConstructor(thenable)` (still observable) and, for `%Promise%`,
  registers a handler-less `Internal` reaction on the thenable: the same single
  job as `then(resolve, reject)` with the unreachable derived promise dropped.
  A non-intrinsic species or any other `then` takes the old path.
- `%Promise%`, `%Promise.prototype.then%` and the original `get
  Promise[@@species]` are realm intrinsics rooted by the realm; the code no
  longer compares against the `Promise` global binding. This also fixes
  `Promise.resolve`/`reject`/`withResolvers` ignoring the subclass constructor
  after `globalThis.Promise = class extends Promise {}`.
- `SpeciesConstructor` skips the call of the species getter only while
  `Promise[@@species]` is still the original accessor.
- Jobs and handlers are ordered exactly as before (one job per settlement).

## Measurement

Host load average 9-20 during runs, so absolute numbers carry noise; the three
columns were run interleaved, median of 5 (wall seconds, `RSS` = max resident).

| benchmark | origin/main | branch | boa 0.22 |
|---|---|---|---|
| `then_chain` (120k thens) | 1.76 s / 607 MB | 0.86 s / 350 MB | 0.63 s / 207 MB |
| `then_chain` (240k thens) | 4.84 s / 1192 MB | 2.56 s / 679 MB | 1.20 s / 371 MB |
| `queue_deep` | 2.71 s / 614 MB | 1.53 s / 357 MB | 0.80 s / 205 MB |
| `doxbee_promise` (N=5000) | 1.50 s / 287 MB | 0.56 s / 157 MB | 0.71 s / 125 MB |
| `doxbee_async` (N=5000) | 0.48 s / 111 MB | 0.27 s / 83 MB | 0.44 s / 62 MB |
| `new_promise` | 2.03 s | 1.56 s | 1.12 s |
| `async_call` | 2.44 s | 1.04 s | 1.48 s |

The absolute numbers on the build host are about 1.5x faster than the issue's
buildbox3 numbers (boa `then_chain` 0.63 s here against 1.02 s there) and
`origin/main` already contained other perf work, so the issue's wall-clock
thresholds were calibrated against a slower baseline; the acceptance
thresholds are met in absolute terms regardless (then_chain <= 2.5 s,
240k-chain RSS <= 700 MB, queue_deep <= 3.6 s, doxbee_promise <= 2.2 s,
doxbee_async <= 1.2 s).

The deterministic gate for "no `create_resolving_functions` under
`promise_then`" is `intrinsic_promise_capabilities_create_no_resolving_functions`
in `src/interpreter/tests.rs`: the promises derived by `then`/`catch`, and an
async function's result promise, carry no `resolving_functions`.

## Not done

- `Promise.all`/`allSettled`/`any`/`race` element functions are exposed to
  `then` and remain function objects; only the per-element `PromiseResolve`
  and `then` got cheaper.
- Under `JSSE_GC_STRESS=3`, `built-ins/Promise/*/capability-executor-called-twice`
  and `then/deferred-is-resolved-value` fail identically on `origin/main`: the
  general-path executor keeps the capability's resolve/reject only in
  `Rc` slots the tracer cannot see. Not part of this change.
