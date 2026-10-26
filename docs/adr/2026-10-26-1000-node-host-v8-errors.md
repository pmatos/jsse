# Node host mode: V8-compatible Error API

Status: accepted (#1228), epic #1214. Extends
`2026-10-26-0854-node-host-mode.md`.

## Context

Node tooling (mocha, source-map-support, depd, callsites, assert) reads
`error.stack`, calls `Error.captureStackTrace`, sets `Error.stackTraceLimit`
and `Error.prepareStackTrace`, and walks CallSite objects. jsse's `stack` is
the engine-defined empty string of the `error-stack-accessor` proposal, and the
interpreter knew no source position for a call. The default environment is what
test262 measures, so everything here is `--node` only.

## Decision

1. **Opt-in by installation.** `scripts/node-v8-error.js`, the first prelude
   script, calls `__host_stack_install(Error, getter, setter)`. Until then no
   native path does anything (`NodeStackState::installed`); without `--node`
   the script never runs. The one process-wide switch, read by the bytecode
   compiler (below), is set by the same call. Default mode keeps
   `Error.prototype.stack` and `new Error().stack === ""`; the prototype
   accessor also stays under `--node` (Node has none), which only shows for an
   error that never captured a stack.
2. **Rust records, JavaScript formats.** `builtins/node_host_stack.rs` captures
   raw frames without running script, so it is safe from the engine's own
   `create_error` (a `TypeError` from `null.x`), from the constructors, and from
   `captureStackTrace`. The prelude installs the lazy `stack` accessor and does
   everything V8 does on first read: the header, `Error.prepareStackTrace`, the
   CallSite class, the `at fn (file:line:col)` text.
3. **Positions.** `CallSiteId` is now `(id, offset + 1)`; the parser stores the
   position V8 reports: the callee's last identifier (`f()`, `o.m()`, as written: a
   parenthesized or escaped callee gets the `(`), else the `(`, in optional chains too; `new` for `new X`. `0` means unknown. Tree-walker calls store it in
   `NodeHostState::call_pos` just before dispatch; the VM does the same with
   `Op::SetCallPos <u32>`, emitted before `Call`/`Construct`/the spread calls
   only while the hooks are installed, so default-mode chunks are byte-identical.
   `run_activation` moves `call_pos` into the new `CallFrame::caller_pos` and
   resets it, and the pop restores it. A frame's position is therefore the
   `caller_pos` of the frame above it, or `call_pos` for the top frame. Line and
   column are computed lazily from the function's `SourceText` (`__host_stack_locate`,
   a per-source line table; columns count UTF-16 units; `\r\n`, `\r`, U+2028/9).
   `CallFrame::env_index` finds the frame's `this` and `new.target` (block
   scopes share `call_stack_envs`, so frame and env indices differ).
4. **Capture.** The error constructors are wrapped after `setup_error_builtins`
   so an instance records its stack on return. `new Derived()` skips frames up
   to its constructor, as V8's `SKIP_UNTIL_SEEN` does (`Until::Constructor`; a
   derived class without a body has no frame here, so its superclass
   constructors go instead). `captureStackTrace(obj, fn)` is `Until::Seen`: an
   `fn` that is not on the stack captures nothing. `Error.stackTraceLimit` is
   read from the `Error` constructor on every capture; a non-number leaves
   `stack` undefined.
5. **Storage.** Frames are a flat array of strings and numbers (name, type name,
   source id, position, flags) in a hidden slot (`PrivateId` from the top of the
   range, as `BOUND_COMPARE`), which the collector already traces; the hook
   functions live in hidden slots of the global object. No native closure holds
   a `JsValue`. The array is written with `gc_write_barrier_value`. `stack` is
   an own, non-enumerable accessor with shared getter and setter functions; the
   first read formats once and replaces it with a data property, like V8 14
   (Node 26): the header uses `name` and `message` as they are at that moment,
   and `prepareStackTrace` runs once, then never again for that error. A
   re-entrancy flag makes nested errors inside `prepareStackTrace` use the
   default format.
6. **File names.** Frames of the main script carry its path
   (`Interpreter::note_script_source`, called from the CLI; the CommonJS
   bootstrap stub is not named); a source with a `//# sourceURL=` directive on a
   line of its own uses it; `__host_stack_name_source(fn, name, lineDelta)` names
   the source of `fn` explicitly. The CommonJS loader (ADR-2026-10-26-1400)
   calls it from `wrapSafe` with the module filename and `-2`, the two header
   lines `Function` adds, so frames of required modules read `file:line:col`
   like Node's `vm.compileFunction` (a leading `#!` line keeps its line). The
   builtin loader captures the primitive into the private
   `internal/host/stack` and deletes the global, like the other floor
   primitives. The wrapper is anonymous (`name` is `""`), so its frame reads
   `Object.<anonymous>`. For this to name nested functions too, the `Function`
   family now gives the wrapper the parser's `SourceText` (one shared origin)
   instead of a copy; this also fixes the wrapper frame's column, which was
   one too high. Builtin module sources (`node-lib/`) stay `<anonymous>` and
   `node:internal/...` naming is future work.
7. **Inspection.** The shim's `util.inspect` renders an error as its stack, then
   own enumerable properties, `[cause]` and `[errors]` in a `{ }` block (`[text]`
   when the stack has no frame), and hides `name`/`message`/`stack` keys the
   stack already shows, as Node's `formatError` does. It calls the lazy getter
   only when it is the property's own getter. Node's `... N lines matching
   cause stack trace ...` elision and its width-based line breaking are not
   implemented (the shim's inspect is best-effort by design).
8. **Hot-path cost.** Default mode pays one `u32` store per tree-walker call and
   a `mem::take` plus a store per activation. `CallFrame` grew by 16 bytes.

## Known gaps (versus Node 26)

- No frame for script top level, `eval`/`Function` code (shown as
  `<anonymous>`), generators, or state-machine async functions; no native
  frames (`at Array.map (<anonymous>)`); no `at async` frames. Hosted async
  functions have their own frame, also after an `await`.
- A frame entered implicitly (getter, setter, `valueOf`, proxy trap, iterator
  protocol) reports the caller's most recent call position; an error the engine
  throws reports the throwing frame's most recent call, or the function start.
- Arrow functions have no receiver type; `getThis`/`getFunction` return
  `undefined`; `getMethodName` is the function name; V8-inferred names
  (`F.s` for `F.s = function(){}`) are not synthesised.
- Tail calls replace frames only as far as the engine already does.

## Verification

`scripts/run-node-v8-error-oracle.sh` runs `scripts/v8-error-fixtures/cases.js`
on Node and on `jsse --node`, normalises paths and drops frames outside the
fixture (and Node's top-level `Object.<anonymous>`), and diffs both against
`expected-cases.txt` (Node 26.8's own output; regenerate with `--node`).
`modules.js` (with `lib/`) does the same for frames of required modules, including a shebang module. `tests/node_v8_error.rs` replays both under the VM, the tree-walker and
`JSSE_GC_STRESS=7`, and adds default-mode absence, engine-thrown errors, spread
and construct positions, `sourceURL` and loader-named module sources, an allocating
`prepareStackTrace` that calls `$262.gc()`, and the uncaught-error printer.
Oversized files this touches stay byte-neutral: edits there are offset by
shortening comments, per the staged-file ratchet.
