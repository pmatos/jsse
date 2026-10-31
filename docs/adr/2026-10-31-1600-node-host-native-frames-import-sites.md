# Node host mode: builtin frames, `import()` call sites and module error text

Status: accepted (#1482, #1483, #1484), epic #1214. Extends
`2026-10-26-1000-node-host-v8-errors.md` (its "no native frames" gap) and
`2026-10-28-1400-node-host-async-frames.md`.

## Context

The hereby stacks oracle (#1251) found three ways `--node` stacks differed from
Node 26: no `at Array.map (<anonymous>)` frame for a builtin that called back or
threw; an error made while `import()` loads a module carried the wrong frames
(the function start for a position, a job's `at async` chain instead of the
caller's live frames); and module syntax and link errors read differently
(`Expected binding pattern, got Assign`, `Module '/abs/lib.mjs' has no export
named 'absent'`, no source line). Everything is `--node` only: the new state
is touched only once the stack hooks are installed, and the parser changes are
gated by the same switch as function name inference.

## Decision

1. **Builtin frames** (`builtins/node_host_stack/native_frames.rs`). A native
   call does not push a `CallFrame`: a dozen readers assume the top one is a
   function activation. While the hooks are installed `run_native_call`
   (`eval/native_call.rs`, the native arm of `call_function`) puts a visible
   builtin on `NodeStackState::natives`, each entry with its function, receiver,
   whether it was a construct call, and `call_stack_frames.len()` at the call.
   `raw_frames` interleaves the two lists: a builtin sits above the activations
   that were live when it was called, and `skip_top`/`Until` hide the builtins
   above the last hidden activation. The position of the activation below a
   builtin needs no change, since the activation the builtin called takes the
   builtin's call position as before. The frame is `[name, receiver type,
   no source]`, which the prelude prints `Array.map (<anonymous>)`, `new
   Promise (<anonymous>)`, `parseInt (<anonymous>)`.
   *Which builtins.* Those that exist before the host floor is built and have a
   name (`note_native_frame_intrinsics`, a bitset by object id); natives made
   later (promise resolvers, host plumbing, the `__host_*` floor) never show.
   V8 runs some builtins without a frame, and so do we: `Function.prototype.
   call`/`apply`, `Reflect.apply`/`construct`, global `eval`, the `Array` and
   `Function` constructors, the error constructors and `structuredClone`
   (JavaScript in Node). `RegExp.prototype[Symbol.replace/match/matchAll/
   search/split]` shows no frame under another builtin (`String.prototype.
   replace` tail calls it).
   *Receiver type.* `receiver_type_name` now names a plain object after its
   `Symbol.toStringTag` data property when the constructor is just `Object`
   (`JSON.parse`, `Math.max`, `Reflect.set`), V8's `GetConstructorName` order.
2. **`import()` positions.** `Expression::Import` carries the offset plus one V8
   reports (the `import` keyword; the `await` when the import is the leftmost
   part of an `await` operand, which V8 reports). The tree-walker stores it in
   `call_pos` before evaluating the specifier, so a call in the specifier
   (`import(pathToFileURL(p).toString())`) leaves its own position behind, as
   V8's latest emitted position does. This is an approximation of V8's rule
   (an `import()` nested in other expressions reports the statement or the
   latest call; here it reports the keyword).
3. **Load errors take the caller's frames** (`node_host_stack/import_site.rs`).
   Node resolves, reads and parses the module graph inside the `import()` call.
   jsse loads in a microtask during module evaluation, so the frames are gone.
   `dynamic_import` captures the caller's frames (`node_import_site`, rooted
   with the job) and the job runs the load under `node_at_import_site`:
   `node_capture_stack` takes those records instead of the live stack for any
   error made meanwhile (resolution, parse). Evaluation of the loaded module and
   the link step stay outside, since Node links in a later job (`at async`).
   `evaluate_dynamic_import` (`eval/import_evaluation.rs`) replaces the two copies
   of the abrupt-evaluation bookkeeping.
4. **Module link errors** (`node_host_stack/link_errors.rs`). A missing named or
   default import reads `The requested module './lib.mjs' does not provide an
   export named 'x'`: the importer's specifier as written and the name's
   position (`ImportSpecifier::Named`/`Default` carry offset plus one) are set
   around the binding by `with_import_name`, and the six former `has no export
   named` sites call `no_export_error`. Node's `decorateErrorStack` prefixes the
   stack with `file:///f.mjs:2`, the source line and carets under the name; the
   same text is kept in a hidden slot of the error and `scripts/node-v8-error.js`
   (`__host_stack_arrow`) prepends it when `stack` is first read. Default mode
   keeps `Module '<path>' has no export named 'x'`.
5. **Parser wording** (`parser/node_hooks.rs`). Once hooks are installed, "this
   token is not allowed here" reads like V8: `Unexpected token '='`,
   `Unexpected identifier 'y'`, `Unexpected number`/`string`/`template string`,
   `Unexpected end of input`. The token text is sliced from the source. Covered:
   `eat`, `eat_semicolon`, the binding pattern, property name and primary
   expression fallbacks, array/object literal separators and the class name.

## Known gaps

Filed as #1523 (parser wording) and #1524 (loader errors).

- Parser messages for productions that have their own V8 text (`Function
  statements require a function name`, `Missing catch or finally after try`,
  `let is disallowed as a lexically bound name`, `Invalid or unexpected token`
  for `@`) and the long tail of the 230 `self.error` sites keep jsse's wording.
  `eval` and `Function` prefix the message with `SyntaxError: ` (both modes).
- `export { x } from './m'` and `export * from` link errors keep the old text and
  have no source line.
- A generator body has no frame, so `Generator.next` is named `Object.next`.
- Importing a module that failed to load again returns the first error object
  with its first stack (Node makes a fresh one).
