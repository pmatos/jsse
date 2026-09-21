# JSSE Engine Context

Domain language for the JSSE JavaScript engine: the modules, concepts, and seams that shape the interpreter and its interfaces.

## Language

**Body**:
A unit of executable ECMAScript syntax — a script, module, or function body — that owns its own IC site map and is the granularity at which inline-cache state is stored.
_Avoid_: function body, script body, code unit.

**IC Site**:
A specific call or property-access location in a Body that can be inline-cached at runtime.
_Avoid_: cache entry, IC slot (when referring to the location rather than the stored value).

**IC State**:
Where a property-access IC Site sits on the `Empty → Mono → Poly → Megamorphic` lattice. `Mono` caches one object; `Poly` caches up to `MAX_POLY_PROP` distinct objects (issue #71); `Megamorphic` is the terminal give-up state. Driven by `PropIcSlot::advance`.
_Avoid_: cache mode, IC level.

**CallSiteId**:
A dense identifier assigned to a call IC site within a single Body.
_Avoid_: call IC index, call cache id.

**PropSiteId**:
A dense identifier assigned to a property-access IC site within a single Body.
_Avoid_: prop IC index, property cache id.

**BodyIcInfo**:
Metadata describing the number and kinds of IC sites in a Body, used to size the runtime IC store without coupling the AST to the runtime slot types.
_Avoid_: cache header, IC metadata.

**BodyIcStore**:
The runtime cache of IC slot values for a Body, keyed by the Body's identity and shared by all closures of that Body.
_Avoid_: cache table, IC map.

**Module Key**:
The canonical host identity of a resolved ECMAScript module, whether it is backed by a file or supplied directly by the host.
_Avoid_: module path, registry path

**Seam**:
A place where one module's interface ends and another's begins. In JSSE, the seams between the AST, the inline-cache system, and the interpreter are intentionally narrow: the AST carries site identifiers, the runtime carries slot values, and the interpreter maps one to the other.
_Avoid_: boundary, layer.

**Divergence Tier**:
How the `differential` fuzz target (`fuzz/fuzz_targets/differential.rs`) classifies a jsse-vs-node run. Tier 1: jsse crashed (signal or the interpreter-panic exit code) while node didn't — an engine bug by definition. Tier 2: exactly one side rejects the source as a syntax error — a real coverage gap. Tier 3: both sides threw (possibly a different error class) or both timed out — expected noise (usually an unimplemented feature), recorded but not a fuzzer finding. See `docs/adr/0004-fuzz-lib-target-and-subprocess-differential.md`.
_Avoid_: divergence class, mismatch level.

## Control flow

**Completion Propagation**:
The unwrap-or-early-return of the interpreter's `Completion` type behind the `propagate!(expr)` macro and its `IntoAbrupt` trait (`src/interpreter/types.rs`): the success value (`Completion::Normal`, or a `Result` `Ok`) is bound, and any abrupt completion early-returns out of the enclosing `-> Completion` function. `IntoAbrupt` adapts the three propagation source shapes into one spelling — a `Completion`, a `Result<T, JsValue>` (a Rust `Err` is a JS throw, wrapped in `Completion::Throw`), and a `Result<T, Completion>` (passed through) — replacing the hand-rolled two-line `match` heads scattered across the natives. `Completion::Empty` is not abrupt but is still propagated verbatim, matching the sites it replaces. A new source shape is one added `IntoAbrupt` impl, with no call-site churn.
_Avoid_: try macro, error unwrap, ReturnIfAbrupt helper.

**Isolated Block**:
A `Block` that directly declares `await using`, emitted intact as the last statement of its own async-function state (`generator_transform.rs`, `Statement::Block` arm). Its block environment and dispose stack stay whole, so `async_function_resume` can park the block's `DisposeCursor` and suspend at each DisposeResources `Await` instead of draining the microtask queue inline. `has_suspendable_await_using_block` (`generator_analysis.rs`) decides which containers (`try`/`catch`/`finally` bodies, loop bodies, `switch` cases, `if`, labeled statements, plain blocks) the transform lowers to reach one; a container whose lowering would flatten an observable lexical scope is left on the tree-walker.
_Avoid_: dispose block, await-using state.

**Block Exits**:
The transform-time table (`GeneratorState.block_exits`, `BlockExits`) of `break`/`continue` targets, keyed by label, in scope where an **Isolated Block** was emitted. The block runs verbatim, so a jump leaving it surfaces as a raw `Completion::Break`/`Continue` after its disposal; the driver resolves it through this table into `route_loop_control!`, which runs intervening `finally` blocks and closes crossed `for-of` iterators. A side table rather than a `StateTerminator` variant so the generator executors stay untouched.
_Avoid_: jump table, loop targets.

## Memory

**Temp-Root Frame**:
A saved depth marker into the interpreter's `gc_temp_roots` stack — the set of `JsValue`s pinned as GC roots only for the duration of one native operation, so a GC safepoint reached while they exist solely as Rust locals cannot collect them. `gc_root_frame` captures the current depth; `gc_unroot_frame` bulk-truncates back to it. A native that roots temporaries opens a frame, roots values into it, and truncates on exit.
_Avoid_: root scope marker, gc stack pointer.

**GC Root Scope**:
The lexical scoping of a Temp-Root Frame behind the `with_gc_root_scope(|interp| …)` combinator: it captures the frame, runs the body, and truncates on every exit path (tail, early `return`, `?`) so the teardown cannot be forgotten on a branch. Prefer it to a hand-paired `gc_root_frame`/`gc_unroot_frame` for a whole-body, single-frame native; reach for the raw primitive only when frames nest or interleave across early exits.
_Avoid_: root guard, unroot epilogue.

**Pinned Native Root**:
A `JsValue` attached to an anchor object's `gc_native_roots` list by `pin_native_root`, so it stays reachable for as long as the anchor is. Unlike a Temp-Root Frame it outlives the native call that created it, which is what a native closure's captures need. Pins only ever accumulate, so an anchor must be pinned to a *fixed* set of values, established once; a value written *after* the pin belongs in a **Rooted Slot** instead.
_Avoid_: permanent root, closure root.

**Rooted Slot**:
A GC-traced container an anchor pins once and the owner then mutates in place, for a capture whose value is *replaced* over the anchor's lifetime. A native closure's own `Rc<RefCell<…>>` state is invisible to the tracer, and re-pinning each replacement would retain every superseded value, so the slot — not the value — is what gets pinned. `RootedSlots` in `gc.rs` is the container: growable and indexable (`push`, `set`, `get`, `snapshot`), backed by an arena object so writes run the generational write barrier, and pinned with `pin_on`. Its consumers are `Iterator.concat` and `Iterator.prototype.flatMap` — through `RootedPair` in `builtins/iterators.rs`, the two-slot adapter holding the iterator currently being drawn from plus that iterator's `next` method — and the accumulators of the Promise combinators (`all`, `allSettled`, `any`, `allKeyed`, `allSettledKeyed`).
_Avoid_: root cell, traced box, rooted buffer.

## Builtins

**Receiver Guard**:
The single prologue a native method routes its `this` through to brand-check the
receiver — or throw the brand `TypeError` — so callers never re-spell the
`as_object_id → get_object → borrow → <kind>_info` dance. The family:
`require_array_buffer` / `require_shared_array_buffer` (`builtins/typedarray.rs`)
hand back an owned snapshot of the buffer's kind-specific info; `with_typed_array_ref`
is the leaner kernel form — it runs a caller-supplied closure against the borrowed
`TypedArrayInfo` and returns whatever the closure returns (a scalar, an owned
snapshot via `TypedArrayInfo::clone`, or anything else), so ownership is the
caller's choice, not the guard's. `validate_typed_array` layers a snapshot +
detached/OOB throw on top of the kernel. A guard may be parameterized by a
detach policy: `ta_number_getter` layers the numeric getters' spec
`TypedArrayLength → 0`-on-detached-or-out-of-bounds rule on top of the kernel,
where `validate_typed_array` instead throws. Either way, any borrow the guard
holds drops before `create_type_error` runs (it mutates the object arena).
_Avoid_: receiver check, brand check (for the whole prologue), this-unwrap.

**Close-on-Reject**:
The exit an `%IteratorPrototype%` helper takes when its argument fails
validation: build the error, close the iterator the helper was handed but does
not own (spec `IfAbruptCloseIterator`), then throw. `require_callable_arg`
(`builtins/iterators.rs`) concentrates it for the eight helpers taking a callable
argument. Two orderings are load-bearing and are the reason this is a seam rather
than a convention: the error object is constructed **before** the close, because
the close runs a user `return()` method that can rebind the global `TypeError`
and so change the thrown error's prototype; and the error is GC-rooted **across**
the close, because the close runs arbitrary JS and the collector does not scan
the Rust stack. The close's own failure is discarded — the original argument
error wins. Distinct from a Receiver Guard, which checks `this` and has no
iterator to close; the two are deliberately separate here, because only 9 of the
14 helpers brand-check their receiver at all.
_Avoid_: close-and-throw, argument check, iterator preamble.
