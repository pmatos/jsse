# V8 receiver, range, proxy and spread messages (#1532, #1512)

Epic #1214. Continues `library-tests-typescript-conformance-v8-messages-1510.md`
(builtin wording, destructuring and iteration text). Everything here is `--node`
only: a message takes V8's wording once the stack hooks are installed
(`node_host.stack.installed`) and keeps the engine's own text otherwise.

## Running

- Oracle (needs `node` 26.8): `./scripts/run-node-v8-messages-1532-oracle.sh`
  (`--no-bytecode`, `--no-cross-check`, `--node` to regenerate the expected files).
- Without Node: `cargo test --profile release-nolto --test node_v8_messages_1532`
  (VM, tree-walker, `JSSE_GC_STRESS=7`, and the default-mode wording).
- Fixtures: `tests/node-v8-messages-1532/*.js` with Node's output in `*.out`;
  remaining differences in `tests/node-v8-messages-1532/known-failures.md`.

## Where the wording lives

- `builtins/node_host_receiver_wording.rs`: `frame_wording`, asked first by
  `node_error_message`. The builtin that is running and its receiver are the
  top of the native frame list (`top_native`, only while no script frame is
  above it), so the throw sites stay as they are: `String.prototype.trim called
  on null or undefined`, `Array.prototype.map called on ...`, `Method
  Date.prototype.toString called on incompatible receiver #<Object>` (Date,
  RegExp, DataView, ArrayBuffer, %TypedArray%), `Promise.all called on
  non-object`, the detached buffer family (`Cannot perform %TypedArray%.prototype.fill
  on a detached or out-of-bounds ArrayBuffer`) and `BigInt.asIntN`. A rule names
  the engine message it rewrites and checks the builtin against the realm's
  intrinsics, so a message made by user code is never touched.
- `builtins/node_host_buffer_errors.rs`: the offset or length in `Start offset 5
  is outside the bounds of the buffer` / `Invalid typed array length: 5` (the
  throw sites in `typedarray.rs` pass the number).
- `builtins/node_host_proxy_errors.rs`: `invoke_proxy_trap` and the `set` trap
  leave a note of the last trap call (key, target, result) under `--node`;
  `proxy_wording` names the property and prints the values in the invariant
  messages. `proxy_trap_error` words a trap that is not a function.
- `builtins/node_host_json_cycle.rs`: the `--> starting at object with
  constructor 'Object'` path of a circular `JSON.stringify`. The serializer
  keeps objects, not keys, so the keys are found again on the error path.
- `builtins/node_host_spread_errors.rs`: call and `new` argument spread of a
  non-iterable. The tree-walker words it in `call_spread_items`; the compiler
  notes the operand text (`Chunk::pattern_src`) before each spread op and
  `vm/spread.rs` reads it. A non-callable `@@iterator` is not iterable
  (`iterator_from_method.rs`, which also holds `Array.from`'s own wording).
- `builtins/node_host_destructure.rs`: `var [a] = five` reads `five is not
  iterable` when the initializer is a name, literal, object literal or call and
  the failing value is the initializer's own (`iterable_value`).
- `builtins/node_host_assign_errors.rs`: strict `undefined = 1` is `Cannot assign
  to read only property 'undefined' of object '#<Object>'`.
- Tagged templates: the parser stores the template's source offset plus one in
  the high 32 bits of `TemplateLiteral::id` (`Parser::template_id`); the
  tree-walker sets the frame's position from it before the tag is called
  (`note_template_pos`), and the callee scanner reads the tag before a backtick,
  so ``o.f`a` `` is `o.f is not a function` at the template. Tagged templates
  always run on the tree-walker (the compiler does not accept them).
- `builtins/node_host_callee.rs`: an object literal callee prints
  `{(intermediate value)}`, and a call made by a builtin (a native frame on
  top) no longer reads a callee from the source at a stale call position.

## Rules

- A new message goes into a fixture, then regenerate the `.out` with `--node`.
  Keep each authored file under 20,000 bytes; files over it may not grow.
- Never let the V8 branch replace an error user code threw.

## Not done

Positions of operator, identifier, TDZ, const-assignment, heritage,
compound-assignment, update, `delete`, `new` chain and optional chain errors
(#1511) need a source offset the AST does not carry; see
`tests/node-v8-messages-1532/known-failures.md` for the design notes and the
other open wording.
