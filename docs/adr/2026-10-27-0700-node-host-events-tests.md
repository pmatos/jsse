# Node host mode: the ported events test set (#1223)

Status: accepted, epic #1214. Extends the test set of the `events` addendum in
`2026-10-26-0854-node-host-mode.md`; the tests listed under "Not vendored"
remain open. This supersedes that addendum's "30 tests" count and its
`needs-process-events` / `needs-inspect` markers, and the claim in
`2026-10-26-1900-node-host-util.md` that the `needs-inspect` tests ran: their
probe could never pass.

## Decision

1. **Vendored set.** `scripts/events-fixtures/manifest.txt` lists Node v26.8.2
   `test/parallel` files, verbatim: 43 CommonJS tests (wrapped by `harness.js`; 37 before item 6)
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
   `test/common/gc.js` and collectable `WeakRef`s;
   `test-eventtarget-memoryleakwarning` needs `MessageChannel` (its expected
   warning text names `[MessagePort [EventTarget]]`) and collectable `WeakRef`s.
   None of those modules exists in the host.
6. **`--expose-internals` (second step, #1223).** `jsse --node
   --expose-internals` lets user `require` (and `node:` ids) name the embedded
   `internal/...` builtins, as Node's switch does; `process`, `module` and the
   default of keeping `internal/*` private are unchanged without it. The CLI flag
   is accepted without `--node` and ignored (the host floor, including
   `__host_expose_internals`, is not installed, so the default global
   environment stays identical); `node-builtin-loader.js` reads the boolean once
   and removes the hook, and only `canBeRequiredByUsers` consults it.
   `module.builtinModules`, `Module.isBuiltin` and `process.execArgv` do not
   mirror the switch. The oracle runs a test with the switch on jsse and Node
   when its `// Flags:` line names `--expose-internals`. Six tests moved in:
   `test-abortcontroller-internal`, `test-events-customevent`,
   `test-events-on-async-iterator`, `test-events-static-geteventlisteners`,
   `test-eventtarget` (two `.partN` slices with a sha256, like the assert
   fixtures, to stay under the 20,000-byte budget) and
   `test-eventtarget-brandcheck`.
7. **`NodeEventTarget`.** The tests reach `NodeEventTarget` and
   `initNodeEventTarget` from `internal/event_target`. They live in
   `internal/event_target_node.js` (the 20,000-byte budget) and
   `internal/event_target` exports them through getters, so key order and
   `destructuring` match Node. The symbols both modules share
   (`kHybridDispatch`, `kIsNodeStyleListener`, `kIsNodeEventTarget`) moved to
   `internal/events/symbols`.
8. **A handled throw from a tick.** `process.on('uncaughtException')` that
   handles an error thrown inside a `nextTick` callback abandons that tick
   checkpoint, as in Node. Node then keeps the next check phase non-empty
   (`setImmediate(noop)` in `createOnGlobalUncaughtException`) so the rest of
   the queue and the promise jobs it feeds run promptly; the prelude now does
   the same (`afterHandledUncaught` in `node-timers-shim.js`). Before, the queue
   waited for an unrelated later callback: with four failing listeners
   `test-eventtarget` lost the fourth `uncaughtException`.
9. **Patched block, jsse only.** `test-eventtarget` has a block-level check that
   a weakly held listener is gone after `gc()` (#1317). A manifest line
   `~test-eventtarget|s#from#to#|reason` (literal text, the assert fixtures'
   format) swaps the `common.mustNotCall()` listener for a no-op on jsse only;
   Node runs the text verbatim. One more subtest is skipped by name
   (`A weak event listener should not prevent gc`).

Addendum (#1317): `$262.gc()` now collects `WeakRef` targets, so the three
`gc` subtests, `A weak event listener should not prevent gc` and the
`~test-eventtarget` patched block above are gone; those tests run verbatim
(ADR-2026-10-28-0200).
