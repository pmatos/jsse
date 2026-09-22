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

**Scope Frame**:
A `Block`, or a `try`/`catch`/`finally` clause's own statement list, that directly declares `await using` in a plain async function, lowered through `EnterScope`/`ExitScope` `StateTerminator`s (`generator_transform.rs`'s `transform_scope_block`/`transform_clause_body`) instead of being flattened. `EnterScope` creates the scope's own `Environment` and pushes a `ScopeFrame { env, try_depth, for_of_depth }` onto `AsyncFunctionState::scope_stack`; every subsequent state body in that scope runs against it (`async_function_resume`'s `term_env`, picking whichever of the innermost scope frame or the innermost `for_of_stack` entry was opened more recently). `ExitScope` pops the frame and disposes its `dispose_stack` suspendably at each DisposeResources `Await`, exactly like the function-level disposal and `for_of_stack`'s own disposals already do. `route_return!`/`route_loop_control!`/the throw-routing block dispose any frames a `return`/`break`/`continue`/throw crosses (`unwind_scopes_to!`) before continuing to route it. Because `scope_stack` is a stack, nested scopes and a scope directly in a try/catch/finally list need no separate mechanism. Gated to plain async functions (`ctx.detect_for_await`) — see ADR-2026-09-21-1007.
_Avoid_: dispose block, await-using state.

**Pattern Lowering**:
How an async function or async generator suspends at an `await` inside a `var`/`let`/`const` object binding pattern (a default initializer or a computed key) instead of letting the tree-walker drain the microtask queue through its blocking `await_value`. `generator_transform.rs`'s `lower_pattern_binding` breaks the pattern into states over temps (`$dstr_src`, `$dstr_key`, `$dstr_val`): one `RequireObjectCoercible`, then per property its computed key at its own position, exactly one property read, and a `ConditionalGoto` on `typeof $val === "undefined"` around the default. Only the parts of a pattern that reach a suspension are broken up; siblings are bound by the tree-walker through a sub-pattern. `generator_analysis.rs`'s `pattern_needs_lowering` gates it, so detection and lowering agree, and only an `await` (not a `yield`) triggers it. Array patterns, catch parameters, for-in/of heads and assignment forms are not lowered yet — see ADR-2026-09-21-2143.
_Avoid_: pattern hoisting, destructuring desugar.

**Frame-Exit Disposal**:
How an *async generator* disposes an `await using` block scope. Unlike a plain async function's **Scope Frame** (`EnterScope`/`ExitScope`), a generator lowers such a block, or a `try`/`catch`/`finally` clause's own statement list, through the ordinary `OpenBlock` scope-depth mechanism (`reconcile_scope_stack`, `generator_scope_stacks`), so the resource registers on that frame's environment. The async-generator driver disposes any frame a state transition leaves (`scope_stack.len() > state.scope_depth`), innermost first, before reconciliation drops it: the request parks at each DisposeResources `Await` (`GeneratorDisposal` in `Interpreter::generator_pending_dispose`, `GeneratorDisposeThen::Reenter`) and re-enters the driver at the state it was about to run. Every exit shape (fall-through, `break`/`continue`, jumps out of `if`/loops/`switch`) from a lowered block is a state transition, so none needs routing of its own (a container with no `await`/`yield` inside is not lowered and still disposes inline, see the ADR); a `return`/throw that completes the generator, or a `.return()`/`.throw()` at a `yield`, disposes all open frames together with the function-level resources (`take_generator_dispose_stack`), and a `for-of` unwind disposes frames nested in the loop before it closes the iterator (`dispose_scopes_inside_for_of`). A disposal reached while a throw or return is already in flight parks like any other: the `DisposeCursor` carries that completion and `async_gen_reenter` restores it. A `.return()` parked in `yield*` disposes the same way, and a `for (await using x of …)` head parks at its iteration environment's disposal; only an inline-yield replay and the `for-of` unwind still fall back to the blocking driver. ADR-2026-09-21-2015, ADR-2026-09-22-2326.
_Avoid_: isolated block, block exits, dispose block, await-using state.

**Inline Yield**:
The degraded fallback for a `yield`/`yield*` the transform leaves inside an expression it does not decompose (a destructuring-assignment default or target, `for (a[yield] of …)`, `super[yield]`). The tree-walker returns `Completion::Yield` out of the state body; the async driver turns it into a synthetic `StateTerminator::Yield` carrying `SentValueBindingKind::InlineYield { yield_target, prev_sent }`, so it suspends through the same terminator tail as a lowered yield — one `Await`, settled from a later job — and an inline `yield*` is handed to the delegate machinery (`inline_yield_delegates`). The resume re-enters the *same* state with `generator_context` fast-forwarding past the yields already taken, so the state's preceding statements and yield operands still run again on every resume (a `yield*` operand does not). A backstop for constructs not yet lowered (#625), not the primary mechanism. ADR-2026-09-21-2157.
_Avoid_: replay yield, fallback yield.

**Terminator Operand**:
An expression a `StateTerminator` carries and the state-machine driver evaluates
down to a single value — a `ConditionalGoto` condition, a `SwitchDispatch`
discriminant or case test, a `ForOfInit` iterable, or a `Yield`/`Await`/`Return`/
`Throw` operand. The three drivers (sync generator, async generator,
`async_function_resume`) share one reading of what the evaluation produced, via
`eval_operand` and the `Operand` enum (`interpreter/eval/operand.rs`):
`Value` / `Throw` / `Suspend` / `Abort` / `Other`. `Abort` carries only
`Completion::Exit`, which every driver must tear down on and propagate verbatim;
`Other` carries `Return`/`Break`/`Continue`/`Empty`, where the drivers
legitimately differ. Routing a throw and the per-driver teardown stay with each
driver — they need its locals and resolve by `continue`/`return` out of its state
loop — so what the seam owns is the classification, the part that had no business
differing.
_Avoid_: terminator expression, operand completion, state operand.

**Loop Control**:
A `break`/`continue` the transform lowers to `StateTerminator::LoopControl(LoopControlTarget)` instead of a bare `Goto`. The target records where the jump lands (`target_state`) and how many `try` contexts (`try_depth`), `for-of` loops (`for_of_depth`) and block scopes (`scope_depth`) remain active there, so routing never depends on state-id equality. The driver routes it through the innermost un-entered `finally` between the jump and its target, closing the `for-of` iterators it crosses first, and resumes the jump when that finalizer's `TryExit` runs (`route_loop_control!` for async functions, `route_generator_loop_control` for sync and async generators). The generator drivers park the jump on the finalizer's `TryContextInfo.pending_loop_control`, so a jump or throw that leaves the finalizer discards it along with the context, and a nested finalizer cannot overwrite it.
_Avoid_: goto, jump state.

**Try Context Pairing**:
The invariant that every `TryEnter` push of a runtime `TryContextInfo` gets exactly one matching `TryExit` pop, on every completion path out of the `try`/`catch` — including a finally-less try/catch's normal completion, which the transform routes through a synthetic `no_finally_exit_state` (`transform_try_statement`) rather than jumping straight to `after_try`. Every depth later computed from the runtime `try_stack` — `LoopControlTarget.try_depth`/`for_of_depth`, and exception-handler search — assumes this pairing; skipping a pop for any path desyncs those depths from the transform's own `try_stack` bookkeeping.
_Avoid_: leaked try context, unpaired pop.

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
