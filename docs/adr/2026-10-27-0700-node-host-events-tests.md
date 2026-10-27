# Node host mode: the ported events test set (#1223)

Status: accepted, epic #1214. Extends the test set of the `events` addendum in
`2026-10-26-0854-node-host-mode.md`; the tests listed under "Not vendored"
remain open. This supersedes that addendum's "30 tests" count and its
`needs-process-events` / `needs-inspect` markers, and the claim in
`2026-10-26-1900-node-host-util.md` that the `needs-inspect` tests ran: their
probe could never pass.

## Decision

1. **Vendored set.** `scripts/events-fixtures/manifest.txt` lists Node v26.8.2
   `test/parallel` files, verbatim: 37 CommonJS tests (wrapped by `harness.js`)
   and 2 ES module tests (`.mjs`). Every one must print `events-fixture: ok` on
   jsse `--node` and on Node, by `scripts/run-node-events-oracle.sh` and
   `tests/node_events.rs`. Added in this step: `test-event-emitter-errors`,
   `-prepend`, `-remove-listeners` (these two use `stream`),
   `test-event-capture-rejections`, `test-events-uncaught-exception-stack`,
   `test-abort-controller-any-timeout`, `test-aborted-util`,
   `test-abortsignal-any.mjs` and `test-events-add-abort-listener.mjs`.
2. **No readiness probes.** The `needs-process-events` and `needs-inspect`
   markers are gone. The inspect probe expected `[Foo]` from
   `util.inspect(new Foo(), { depth: -1 })`, which Node does not print (it
   prints `Foo {}`), so the three max-listeners warning tests were skipped
   forever; they now run (`process.on('warning')` receives the real
   `[EventEmitter]` text).
3. **Harness changes.** The test body runs inside `try { ... } finally {
   __eventsFinish(); }` so a test that throws to an `uncaughtException`
   handler still gets verified. `require` falls through to the real loader for
   any module (stream, util, child_process, ...); the harness no longer reads
   the legacy `globalThis.util` shim, which lacks `inspect.custom` and is not
   a global in Node scripts. `.mjs` tests get their `../common/index.mjs` and
   `node:test` specifiers rewritten at run time to `esm-shim.mjs`
   (`mustCall`, `mustNotCall`, `describe`, `it`); the vendored text is not
   edited.
4. **Skipped subtests (jsse only, by name in `__eventsJsseSkips`).**
   - `Aborted with gc cleanup`, `AbortSignal.timeout() does not prevent the
     signal from being collected`, `AbortSignal with a timeout is not collected
     while there is an active listener`: `$262.gc()` never collects `WeakRef`
     targets (#1317; the spec allows it).
   - `Does not hang forever` (`test-aborted-util`): spawns `process.execPath
     --input-type=module` and feeds the module on stdin; jsse has no
     `--input-type` flag.
5. **Not vendored (`!` lines in the manifest).** `test-abortsignal-cloneable`
   needs `MessageChannel`; `test-eventemitter-asyncresource` needs
   `async_hooks`; `test-event-emitter-no-error-provided-to-error-event` needs
   `domain`; `test-abortsignal-drop-settled-signals.mjs` needs
   `test/common/gc.js` and collectable `WeakRef`s; seven tests
   (`test-abortcontroller-internal`, `test-events-customevent`,
   `test-events-on-async-iterator`, `test-events-static-geteventlisteners`,
   `test-eventtarget`, `test-eventtarget-brandcheck`,
   `test-eventtarget-memoryleakwarning`) run under `--expose-internals` and `require('internal/event_target')`; the
   host keeps `internal/*` private (`2026-10-26-1400-node-cjs-loader.md`), as
   the assert oracle does for `test-assert-myers-diff`. An `--expose-internals`
   switch would unlock them.
