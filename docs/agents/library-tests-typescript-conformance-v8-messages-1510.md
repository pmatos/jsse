# V8 builtin messages, destructuring and iteration text (#1510, #1512)

Epic #1214. Continues `library-tests-typescript-conformance-v8-messages-1419.md`
(callee text and member positions). Everything here is `--node` only: a message
takes V8's wording once the stack hooks are installed (`node_host.stack.installed`)
and keeps the engine's own text otherwise.

## Running

- Oracle (needs `node` 26.8): `./scripts/run-node-v8-messages-1510-oracle.sh`
  (`--no-bytecode`, `--no-cross-check`, `--node` to regenerate the expected files).
- Without Node: `cargo test --profile release-nolto --test node_v8_messages_1510`
  (VM, tree-walker, `JSSE_GC_STRESS=7`, and the default-mode wording).
- Fixtures: `tests/node-v8-messages-1510/*.js` with Node's output in `*.out`;
  remaining differences in `tests/node-v8-messages-1510/known-failures.md`.

## Where the wording lives

- `builtins/node_host_builtin_errors.rs`: `create_error` (moved out of
  `date.rs`) rewrites an operand-free engine message through `static_text`
  (exact table plus the `Reflect.x`, `Number/Symbol/BigInt.prototype.x`,
  typed array constructor, RegExp getter and regexp-syntax families), and
  `v8_plain_text` / `v8_call_site_text` print values as V8 does.
- `builtins/node_host_operand_errors.rs`: errors that print an operand
  (`callback_error`, `bad_this`, `to_index_for`, `not_iterable_error`, ...).
  A throw site calls one of these instead of `create_type_error`.
- `eval/set_rejection.rs`: strict-mode `[[Set]]` failures (read-only, not
  extensible, getter-only) and primitive receivers.
- `helpers/json_v8_errors.rs`: when the engine's `JSON.parse` rejects a text,
  a scanner words the first error like V8's `JsonParser` (position, line and
  column, token context). It stops following nesting at 1000 levels and keeps
  the engine message then.
- `builtins/node_host_expr_text.rs`: V8's `CallPrinter` over the AST, for the
  initializer in `Cannot destructure property 'a' of 'o.p' as it is undefined.`
  and for `x is not iterable`.
- `builtins/node_host_destructure.rs`: `bind_declarator`, `assign_pattern_value`
  and `require_destructurable` check a null or undefined value where the
  initializer is at hand; a nested pattern reads like a property read
  (`bind_pattern` counts open calls under `--node`). The bytecode compiler notes
  a `let`/`const` object pattern's initializer text and a for-of operand's
  message in `Chunk::pattern_src` (only with the hooks on); the VM reads it
  when the read or `GetIterator` fails. Array literal spreads and for-of reword
  the builtin not-iterable error (`iterable_error` marks it).
- `builtins/node_host_callee.rs`: the callee scanner also reads parenthesised
  callees (`(void 0)`, `(0 , o.f)`, `(o.f || o.g)`, function literals as
  `(intermediate value)`) and string literals.

## Rules

- A new message goes into a fixture, then regenerate the `.out` with `--node`.
  Keep each authored file under 20,000 bytes; files over it may not grow.
- Never let the V8 branch replace an error user code threw (see `robust.js`).

## Not done

Builtin frames (`at Array.map (<anonymous>)`) come from #1525. Positions of
operator, identifier, heritage, destructuring and iteration errors (#1511)
need position fields on `Binary`, `Identifier` and `Assign`; tagged template
call positions are open under #1512. See `known-failures.md`.
