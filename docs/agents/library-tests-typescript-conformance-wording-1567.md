# V8 wording residuals under `--node` (#1567, #1568, #1557, #1523)

Epic #1214. Continues `library-tests-typescript-conformance-v8-messages-1557.md`.
Everything here is `--node` only: a message takes V8's wording once the stack
hooks are installed (`node_host.stack.installed`, `inferred_names::enabled()` in
the parser) and keeps the engine's own text otherwise.

## Running

- Oracle (needs `node` 26.8): `./scripts/run-node-v8-messages-1567-oracle.sh`
  (`--no-bytecode`, `--no-cross-check`, `--node` to regenerate the expected files).
- Without Node: `cargo test --profile release-nolto --test node_v8_messages_1567
  --test node_syntax_tail_1567` (VM, tree-walker, `JSSE_GC_STRESS=7`, and the
  default-mode wording).
- Fixtures: `tests/node-v8-messages-1567/*.js` with Node's output in `*.out`;
  `scripts/node-stack-fixtures-1567/tail-1.tsv` with `expected-tail-1.txt`
  (the driver is `scripts/node-stack-fixtures-1523-tail/syntax-tail.mjs`).
  What still differs: `tests/node-v8-messages-1567/known-failures.md`.

## Where the wording lives

- `builtins/node_host_iteration_1567.rs`: `for await` and `yield*` operands.
  `for await` reads `Symbol.asyncIterator`, so a null or undefined operand is
  `Cannot read properties of null (reading 'Symbol(Symbol.asyncIterator)')`; a
  call is `o.m(...) is not a function or its return value is not async
  iterable`, anything else `x is not async iterable` (`undefined is not a
  function` where `CallPrinter` finds no text: `?:`, `&&`, `||`, optional chains,
  sequences of three). `yield*` words the operands whose position V8 finds
  (calls, members, operators, `await`) as `yield* (intermediate value) is not
  [async ]iterable`; names, literals and `this` keep the value form, and in an
  async generator they are `undefined is not a function`. The generator lowering
  keeps an operand with an `await` in a `$forof_iterable_N` / `$yield_val_N`
  temporary, read as `(intermediate value)`. The sites call `await_iterator`,
  `yield_iterator` and `reword_head`.
- `builtins/node_host_callee_context.rs`: a call whose callee is not callable
  and which is the operand of `for await` or `yield*` (the sync iteration
  operands are `node_host_iter_operand.rs`). The context is read from the
  source around the call, like the callee (`node_host_callee.rs`).
- `builtins/node_host_callee_strings.rs`: string keys in a callee are printed
  cooked (`o['\\']()` is `o.\ is not a function`).
- `builtins/node_host_param_patterns.rs`: `Leak` is what an earlier object
  pattern parameter leaves in V8's printer for an array pattern parameter: the
  last default of a nested pattern (`{ a: { c } = {} }` leaves `{}`), or the
  callee of a call default on a plain name; a computed key ends the effect. An
  array pattern parameter with a call default reads `f is not iterable (...)`.
- `builtins/node_host_default_patterns.rs`: a nested pattern with a default
  words its failure by the text of the default (`var { a: { c } = nul } = {}`
  is `Cannot destructure property 'c' of 'nul' as it is null.`, `{ a: [c] = nul
  }` is `nul is not iterable`), whether the default or the value was nullish.
  A parameter's own default keeps the desugared wording of `bind_param`.
- `builtins/node_host_intl_receiver.rs`: `Method Intl.X.prototype.m called on
  incompatible receiver #<Object>`; `format` and `resolvedOptions` of
  `NumberFormat`/`DateTimeFormat` unwrap an object receiver first and name
  `UnwrapNumberFormat`/`UnwrapDateTimeFormat` (the receiver prints as
  `undefined` for `NumberFormat`).
- `parser/v8_messages/regexp_reason.rs`: `\k`, `\p` and a `\c` in a class in
  unicode mode have their own reasons. `parser/node_hooks.rs`:
  `for-await-of loop variable declaration may not have an initializer.`, a line
  break before the `=>` of `async a`, `yield` as a name in a generator.

## Rules

- A new message goes into a fixture generated from Node
  (`scripts/run-node-v8-messages-1567-oracle.sh --node`). Keep each authored
  file under 20,000 bytes; files over it (`eval.rs`, `exec.rs`,
  `generator_runtime.rs`, `regexp.rs`, `expressions.rs`) may not grow, so a hook
  replaces a call by one of the same size or less.
- Never let the V8 branch replace an error user code threw: the rewording
  consumes the `iterable_error` mark of the frame, which only the engine's own
  not-iterable error carries.
