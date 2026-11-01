# V8 wording of iteration operands, parameter patterns, proxy traps and builtins (#1557, #1558)

Epic #1214. Continues `library-tests-typescript-conformance-v8-messages-1512.md`
and `...-1532.md`. Everything here is `--node` only: a message takes V8's
wording once the stack hooks are installed (`node_host.stack.installed`) and
keeps the engine's own text otherwise.

## Running

- Oracle (needs `node` 26.8): `./scripts/run-node-v8-messages-1557-oracle.sh`
  (`--no-bytecode`, `--no-cross-check`, `--node` to regenerate the expected files).
- Without Node: `cargo test --profile release-nolto --test node_v8_messages_1557`
  (VM, tree-walker, `JSSE_GC_STRESS=7`, and the default-mode wording).
- Fixtures: `tests/node-v8-messages-1557/*.js` with Node's output in `*.out`;
  remaining differences in `tests/node-v8-messages-1557/known-failures.md`.

## Where the wording lives

- `builtins/node_host_iter_operand.rs`: V8 words an error by the expression at
  the error position, and an iteration operand (for-of head, array spread,
  initializer of an array pattern, source of an array assignment) is printed
  whole. `eval_iter` evaluates such an operand and rewords what its own
  top-level operation threw: a call that is not callable reads `o.m is not a
  function or its return value is not iterable` (`not_callable_error` notes the
  error, its call depth and position; a nested call has another position), an
  array assignment that cannot iterate its source reads `[b] is not iterable`.
  The bytecode compiler records the positions of for-of call operands in
  `Chunk::iter_calls` and `Op::Call` rewords through `call_abrupt`; a tagged
  template or array assignment operand keeps the whole for-of on the
  tree-walker. `for await` and `yield*` use other wording and are untouched.
- `builtins/node_host_param_patterns.rs`: an object pattern parameter with a
  default prints `(intermediate value)(intermediate value)(intermediate
  value)` as the initializer; an array pattern parameter after an object
  pattern parameter that has no default, nested pattern or property default
  reads like a property read of that parameter (`bind_param` notes it before
  the parameter is bound). `PatternKind` tells a default's wrapper from the
  pattern it wraps.
- `builtins/node_host_expr_text.rs`: V8 folds arithmetic and bitwise operators
  on number literals, `-`, `+`, `~` of one and `!` of any literal while
  parsing, so `for (x of 1 + 1)` reads `2`; a left-nested run of one
  arithmetic or bitwise operator prints flat (`(a + b + c)`); `x++` and a
  template with substitutions print.
- `builtins/node_host_destructure.rs`: a nested array pattern whose source is
  any expression reads that expression's text (`Cannot destructure property
  'Symbol(Symbol.iterator)' of '5' as it is undefined.`). The parser keeps the
  parentheses of an identifier initializer of an array pattern as a one-element
  sequence (`declarator_list.rs`, only once the Node prelude installed its
  parse hooks) because V8 words `var [a] = (nul)` by the value.
- `builtins/node_host_callee.rs`: the scanner skips string literals when it
  matches brackets (`a["]"]()`), a `(` after a keyword is a group, and a `new`
  expression as a callee reads `(intermediate value)`.
- `builtins/node_host_wording_1558.rs`: rules over the builtin that is running
  (`top_native`) for SharedArrayBuffer, array, string and RegExp String
  iterator receivers, `Iterator.from`, Intl option ranges and `Collator`
  receivers, the immutable `Object.prototype`; helpers for private fields
  initialized twice, `FinalizationRegistry`, `structuredClone()`
  (`ERR_MISSING_ARGS`) and the flags that follow a RegExp pattern in a syntax
  error.
- `builtins/node_host_proxy_errors.rs`: the trap note covers every trap, and
  `proxy_tail_1558` words the invariants that name the property or print more
  values; `apply` and `construct` traps that are not functions read the call
  site (`p is not a function`, `Reflect.apply is not a function`).

## Engine fix found on the way

`invoke_proxy_trap` kept neither the proxy nor its arguments on the collector's
roots, so a collection during a trap freed the target of a proxy that was a
temporary and the invariant check found nothing to compare
(`test262-extra/Proxy-trap-call-proxy-and-target-gc-rooting.js`).

## Rules

- A new message goes into a fixture generated from Node
  (`scripts/run-node-v8-messages-1557-oracle.sh --node`). Keep each authored
  file under 20,000 bytes; files over it (`eval.rs`, `exec.rs`, `vm.rs`,
  `compiler.rs`, `regexp.rs`) may not grow, so a hook replaces a call by one of
  the same size or less.
- Never let the V8 branch replace an error user code threw.
