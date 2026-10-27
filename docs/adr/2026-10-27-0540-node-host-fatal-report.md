# Node host mode: the fatal-error report

Status: accepted (#1229), epic #1214. Extends
`2026-10-26-0854-node-host-mode.md` and
`2026-10-26-1000-node-host-v8-errors.md`.

## Context

An uncaught exception or unhandled rejection under `--node` must print what
`node` prints to stderr and exit with the same status. Node's report is

```
<file>:<line>
<source line>
<spaces>^

<util.inspect of the error>

Node.js vX.Y.Z
```

for an Error, `\n` plus the same header and the raw value (primitives, with a
`(Use \`node --trace-uncaught ...\`` hint) or `util.inspect` of it (objects)
for anything else, and a fixed `UnhandledPromiseRejection` error for a
rejection with a non-error reason. jsse printed the stack, `Uncaught ...` for
non-errors, no header, and its loader frames read `<anonymous>:N:M`.

## Decision

1. **The text is built in JavaScript**, in `node-lib/internal/process/fatal.js`
   (`report`, `unhandledRejectionError`). The event-loop shim only orders the
   steps: like Node's `process._fatalException` it sets `process.exitCode`,
   emits `exit` (a listener may change the status), then prints, then exits
   with `process.exitCode`. A throwing `uncaughtException` handler prints the
   report and exits with 7 directly: `exit` is not emitted. `uncaughtExceptionMonitor` is emitted first. The builtin
   loader hands the module to the shim through the one-shot symbol
   `jsse.node.fatal.handoff`; before it runs the shim falls back to the bare
   stack.
2. **Error versus value.** `isNativeError` selects the error form, as the
   [[ErrorData]] slot does in Node (an object with an own `stack` string is
   not one). The body is `util.inspect(error, { customInspect: false, depth: 5 })`
   from the embedded util, so own properties, `[cause]`, `[errors]` and the
   `... N lines matching cause stack trace ...` elision come from Node's code.
   Colours are never used.
3. **The header's position.** Node reports the V8 message location: the
   `throw` keyword for a synchronous throw (even of an error built elsewhere,
   and of primitives, which have no stack), the top stack frame for a
   rejection. jsse records the first with a throw site: the parser keeps the
   offset of the `throw` keyword in `Statement::Throw(expr, at)`; the
   tree-walker and the VM (`Op::SetCallPos` before `Op::Throw`, emitted only
   once the stack hooks are installed, so default chunks are byte-identical)
   call `Interpreter::node_note_throw`, which keeps the thrown value, the source
   and the offset in a list of the last four throws on the interpreter (traced
   by the collector, so up to four thrown objects stay alive), so a throw
   caught in a `finally` or cleanup does not hide the outer one. `__host_stack_throw_site(value)` answers only for a value in that list
   (SameValue); otherwise the header uses the error's own top frame
   (parsed from its stack). Throws inside embedded `node:` modules never
   replace the site, so the CommonJS loader's `catch (e) { throw e }` keeps the
   user's position (V8's `finally` rethrow does the same). A throw in the
   chunk compiled with hooks on always sets the position first (0 when the
   parser had none), so `Op::Throw` never reads a stale call position. The header is
   omitted when the position lies in a `node:` source, in `<anonymous>` code
   or when the source cannot be read. The excerpt is read with the unpatched synchronous `fs`
   (`internal/host/fs`), not from the engine's source table, so a file changed
   after loading shows its current text. The caret copies tabs, as Node does.
4. **Unhandled rejections.** A reason is an error only with an own
   `stack` (Node's `isErrorLike`); any other becomes the
   `UnhandledPromiseRejection` error with `code: 'ERR_UNHANDLED_REJECTION'`.
   The reason is rendered with V8's `NoSideEffectsToString` (`#<Object>`,
   `[object Array]`, function source, `Symbol(x)`). Its stack and header are
   Node 26.8's literal internal frames (`node:internal/process/promises:322`),
   which is exact because `process.version` is that release. An Error reason
   is reported with its own stack and header, the exit status is 1.
5. **Internal frames.** Embedded modules are named `node:<id>`
   (`__host_stack_name_source` in the builtin loader), the prelude scripts and
   the main-module stub carry `//# sourceURL=node:internal/...`, so no
   `<anonymous>:N:M` frame is left. Their line numbers are jsse's own.
   Anonymous functions called as a method are named after the property that
   holds them (`Timeout._onTimeout`, `Module._compile`), which V8 infers;
   `process.constructor.name` is `process`.

6. **The ES module entry point.** Node runs an ES main in an async loader, so
   its failure is a rejection delivered to `triggerUncaughtException(err, true)`,
   not a synchronous throw. jsse does the same: `Interpreter::node_uncaught_entry`
   calls the shim's `triggerUncaught(error, 'unhandledRejection', hops)`, which
   waits `hops` microtask turns first (4 after a synchronous throw, one more
   after a failing top-level `await`; measured on Node 26.8, stable over runs and
   chain lengths), so the microtasks the module queued run before `exit` and the
   report, and the tick and immediate queues never do. The origin is
   `unhandledRejection` for `uncaughtExceptionMonitor` and `uncaughtException`,
   no `unhandledRejection` event fires, and a non-error value is not wrapped:
   the report is the loader's literal header (`node:internal/modules/run_main:107`
   / `triggerUncaughtException(`) and the raw value. An error's header follows
   its top frame, as for any rejection. A failing `await` of the main module was
   also delivered a second time through the first run's completion;
   `NodeEsm::entry_reported` stops that.
7. **Module top-level frames.** A module body has no function, so its errors had
   no frame (`[Error: m]`). `builtins/node_host_module_frame.rs` runs each module
   body, and every resumption of a top-level `await`, under a native stand-in
   frame (the `vm` script mechanism), named by the module's `file:` URL; the
   parse origin of each module source is recorded where it is parsed. The async
   state-machine rewrites in `generator_transform.rs` keep a call's source
   position (they used to reset it, so calls after a `await` reported 1:1).

## Known differences from Node

- Internal frames: their names, count and line numbers differ from Node's.
  The oracle drops every `    at ... node:...` line.
- An engine-thrown error (`null.x`) has the V8-errors gap (position of the
  throwing frame's latest call), so its caret column is not Node's.
- A throw at script top level (`-e`, the REPL) has no frame, so no throw site
  and no header; `-e` does not print `[eval]:1`.
- The microtask turns before an ES entry point's report are those of a module
  with at most one failing top-level `await` chain; a failure inside a
  dependency's top-level `await` was not measured.
- Arrow functions have no receiver type, so `process.<anonymous>` reads
  `<anonymous>` (fixtures use `function` handlers).
- A throw inside a generator or an async state machine, or followed by a later
  throw of another value before the report (only the last throw is
  remembered), falls back to the error's top frame. Positions in `eval` code
  are resolved against the enclosing function's source, as call positions are
  (V8-errors gap).
- Colours are never used, though Node colours the report on a terminal or with
  `FORCE_COLOR`.
- Under `--no-bytecode` an Error made in an async function's synchronous part
  lacks that function's frame (#1358), so `fatal-reject-async` is skipped in the
  tree-walker run of `tests/node_fatal_report.rs`.
- Nothing is printed for `--trace-uncaught`, `--report-*` or inspector options.

## Verification

`scripts/run-node-event-loop-oracle.sh` runs every `*.fixture.js` and
`*.fixture.mjs` (ES entry point; `*.helper.mjs` are its imports) on both engines and
compares stdout and the exit status, then stderr in full with
`scripts/event-loop-fixtures/<name>.stderr` after normalising the fixture
directory (`<dir>`), the version line, `node:` frames and the `N` of the cause
elision. Node's output must normalise to the same file (`--update-stderr`
regenerates it from Node). `tests/node_fatal_report.rs` replays the
expectations without Node under the VM, the tree-walker and `JSSE_GC_STRESS=7`.
