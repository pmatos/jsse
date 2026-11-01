# Library Tests: residual V8 wording rows of iteration heads and nested defaults (#1572)

Part of epic #1214 (tracker #1418). `--node` only. Follows
[library-tests-typescript-conformance-wording-1567.md](library-tests-typescript-conformance-wording-1567.md) and
`tests/node-v8-messages-1567/known-failures.md`, whose first four rows this closes.

## What changed

| Row | Node 26.8 | Where |
| --- | --- | --- |
| `for await (x of obj?.m())`, `yield* obj?.m()`, `for (x of obj?.m())`, also `obj.m?.()`, `obj?.["m"]()`, `f?.()`, `obj?.m?.()` | `obj?.m is not a function` (for await, async `yield*`), `obj?.m is not iterable (cannot read property Symbol(Symbol.iterator))` (for-of, sync `yield*`), the callee with the `?.` as written | `optional_call_callee` (`node_host_expr_text.rs`) prints the callee of a chain that ends in a call; `node_host_iteration_1567.rs` uses it in `reword_delegation`, `reword_head` and (for the bytecode, which notes the text at compile time) `head_message` |
| a call of a non-callable inside such a chain in a `for await` head | `obj?.k is not a function`, without the `or its return value` suffix | `iterable_call_message` leaves an optional-chain callee to the plain message |
| `for await (x of new f())`, `yield* new f()` with a non-constructor | `(intermediate value) is not a constructor` / `yield* (intermediate value) is not a constructor`; a plain for-of keeps `f is not a constructor` | `iterable_new_text` (`node_host_callee_context.rs`), asked by `node_callee_text_any(true)`; `new_expression_end` finds the end of the `new` so the same operand test as for calls applies |
| `for await (x of obj.ag)` with `obj.ag = { [Symbol.asyncIterator]: 1 }` | `obj.ag is not async iterable`; for an operand V8 does not print (an object literal) `number 1 is not a function` | `method_not_callable` (`node_host_iteration_1567.rs`) recognises the not-callable error `get_async_iterator` made for the method |
| `var [[c, [d]] = nul] = []`, `var { a: [c, [d]] = nul } = {}`, `[[c] = o.q]` | the default's text: `nul is not iterable`, `o.q is not iterable` | `finish_default_pattern` no longer requires the array pattern to be flat: the error must be the pattern's own (`iterable_error_was_for` the value, or the value was undefined and the default ran), a nested failure keeps its value form (`[[c, [d]] = [1, 2]]` stays `number 2 is not iterable ...`); a member default is worded too |
| `({ a: { c } = nul } = {})`, `[[c] = nul] = []`, `for ({ a: { c } = nul } of c)` | `Cannot destructure property 'c' of 'nul' as it is null.`, `nul is not iterable` | `note_default_target` / `finish_default_target` (`node_host_default_patterns.rs`), called around `with_destruct_lref` of `destructure_object_assignment` and `destructure_array_assignment`; `require_assignable` reads the noted text like `require_destructurable` does for bindings |

`tests/node-v8-messages-1572/` holds four scripts (`iteration`, `optional-calls`, `defaults-nested`, `defaults-members`;
121 cases) with Node's output (`scripts/run-node-v8-messages-1572-oracle.sh [--node]` regenerates it);
`tests/node_v8_messages_1572.rs` replays them on the VM, the tree-walker and under `JSSE_GC_STRESS=7`, and checks that
default mode keeps its own wording. The 1557, 1567 and 1419 to 1532 suites still pass.

## Not met

The three V8-only behaviours of the issue are not wording and cannot be met without breaking the specification, so they stay:

- module `import defer * as ns from './lib.mjs'`: V8 rejects it with `Unexpected token '*'` because it does not ship
  import defer; test262 (feature `import-defer`) requires it to parse and run.
- `using x = 1`: V8 `Unexpected identifier 'x'` because it does not ship explicit resource management; test262
  (feature `explicit-resource-management`) requires the declaration.
- `'use strict'; for (f() of c);`: V8 parses it and throws a runtime ReferenceError (web compatibility); the
  specification makes it an early error (test262 `language/statements/for-of`).

Gating a test262 feature off for `--node` would be a host-specific language, which no rule here allows.

Also still different (neither is a row of the issue):

- A non-callable `Symbol.iterator` method (`for await (x of { [Symbol.iterator]: 1 })`, `yield*` in an async generator,
  a plain for-of): V8 `number 1 is not a function` / `object is not iterable (cannot read property
  Symbol(Symbol.iterator))`, jsse words the operand.
- `[...obj?.m()]`: V8 `(intermediate value) is not iterable`, jsse the value form.
