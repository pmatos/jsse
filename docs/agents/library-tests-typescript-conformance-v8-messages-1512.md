# V8 destructuring, iteration, callee and builtin wording (#1512, #1532)

Epic #1214. Continues `library-tests-typescript-conformance-v8-messages-1532.md`.
Everything here is `--node` only: a message takes V8's wording once the stack
hooks are installed (`node_host.stack.installed`) and keeps the engine's own
text otherwise.

## Running

- Oracle (needs `node` 26.8): `./scripts/run-node-v8-messages-1512-oracle.sh`
  (`--no-bytecode`, `--no-cross-check`, `--node` to regenerate the expected files).
- Without Node: `cargo test --profile release-nolto --test node_v8_messages_1512`
  (VM, tree-walker, `JSSE_GC_STRESS=7`, and the default-mode wording).
- Fixtures: `tests/node-v8-messages-1512/*.js` with Node's output in `*.out`;
  remaining differences in `tests/node-v8-messages-1512/known-failures.md`
  (#1557, #1558).

## Where the wording lives

- `builtins/node_host_destructure.rs`: `Lead` is the first property of an
  object pattern and whether V8 reads it as a property read. A default
  (`{ a = 1 } = null`), a member target (`{ a: o.x } = null`) or a non-name
  rest print `Cannot read properties of null (reading 'a')`; a numeric key
  prints its number; a nested object pattern in an assignment and a for-of
  assignment head read their first property (`require_assignable`).
  `array_pattern_text` lists the initializers V8 prints for an array pattern
  (names, literals, `this`, operators, calls, `new`, tagged templates); a
  member read or an assignment reads as the value.
- `builtins/node_host_pattern_origin.rs`: `PatternOrigin::Catch` prints `.catch`
  like `.for`, and `PatternOrigin::Target` carries the source of `{ a: [b] } =
  source` so `nested_array_assign` words the array pattern by it.
- `builtins/node_host_expr_text.rs`: an assignment prints its target, a
  template without substitutions its string, a one-element sequence (a
  parenthesized object, array or assignment) its element.
  `Interpreter::iterable_message` is `None` where V8 prints the value instead
  (a call of a function expression, an optional chain).
- `builtins/node_host_spread_errors.rs`: only a sole spread in the last
  argument is V8's CallWithSpread (`Spread syntax requires ...`); any other
  spread reads like an array literal, the value form. The compiler notes the
  operand text only in the sole-last case (`note_call_spread_source`).
- `builtins/node_host_callee.rs`: when the scanner does not follow a callee
  (`(five + 1)()`, `(o.a = 1)()`, `(c ? 1 : 2)()`, `[]()`), `ast_text` parses
  the fragment with the real parser and prints it with `expr_text`.
- `builtins/node_host_receiver_wording.rs`: getters of `ArrayBuffer`,
  `DataView` and `%TypedArray%` on a wrong receiver, `%TypedArray%.from/of`
  with a non-constructor, Map/Set iterators and generators, `RegExp.prototype.flags`,
  the `__proto__` setter and `Promise.withResolvers`.
- `builtins/node_host_heritage.rs`: `Super constructor null of A is not a
  constructor`. `builtins/host.rs`: engine DOM exceptions (`atob`,
  `structuredClone`) are instances of the host `DOMException`.
  `builtins/node_host_symbol_wording.rs`: ToBigInt of a value and a
  non-callable `@@hasInstance`.

## Rules

- A new message goes into a fixture generated from Node
  (`scripts/run-node-v8-messages-1512-oracle.sh --node`). Keep each authored
  file under 20,000 bytes; files over it (`eval.rs`) may not grow, so a hook
  replaces a call by one of the same size or less.
- Never let the V8 branch replace an error user code threw.
