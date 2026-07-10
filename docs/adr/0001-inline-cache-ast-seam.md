# Inline-cache state lives in the interpreter, not the AST

The AST used to carry inline-cache (IC) slots directly on `Expression::Call`, `Expression::New`, and `Expression::Member` nodes via `Cell<CallIcSlot>` and `Cell<PropIcSlot>`. That forced `src/ast.rs` to import runtime cache types from `src/interpreter/ic.rs`, polluted the syntax module with mutable runtime state, and made it hard to share cache state across closures of the same function body.

We decided to move all IC slot values into the interpreter and let the AST publish only dense site identifiers. A `Body` wrapper around `Rc<Vec<Statement>>` carries `BodyIcInfo` (the number of call and property sites). A shared `ast::assign_ic_sites` pass, run by the parser, generator transform, `eval`, and `new Function`, numbers every `CallSiteId` and `PropSiteId` within a body. The interpreter keeps a body-pointer-keyed side table (`IcStore`) that creates a `BodyIcStore` on first execution and shares it across all closures of that body. The interpreter carries a `current_ic_handle` field that is updated at body entry (`exec_body`) and restored at exit; the hot path in `eval_call`/`eval_member` reads or writes slots via `Interpreter::call_slot`/`prop_slot` by site id. This avoids threading a store reference through every `eval_expr` call site while preserving the same per-body cache semantics.

We chose the minimal concrete interface over a generic `IcStorage` trait or putting the store inside `Body`. A trait seam would add generic plumbing throughout the evaluator before we have a second production adapter; putting the store inside `Body` would move the runtime cache types back into the AST module, defeating the purpose of the change. The minimal interface keeps the seam small: `IcStore::for_body`, `BodyIcStore::call_slot`, and `BodyIcStore::prop_slot`.

## Consequences

- `src/ast.rs` drops its dependency on `interpreter::ic` and shrinks `Call`/`New`/`Member` nodes by replacing 32–40 byte `Cell<...>` fields with 4-byte site ids.
- The parser, generator transform, and dynamic compilation paths must produce a `Body` and call `assign_ic_sites` before execution.
- `exec_body` enters a `Body` by fetching its handle from `IcStore` and installing it as `current_ic_handle`, then restores the previous handle on exit. Script, function, generator, async, and `eval` bodies all route through `exec_body`.
- Module top-level executable items are not stored in a `Body`, so `ast::assign_ic_sites_for_module` assigns them a single dense namespace keyed by the program's `body` field and `execute_module_body_sync` installs that handle around item execution.
- The interpreter gains an `IcStore` field holding the per-body side table; the table pins the body `Rc` to avoid ABA reuse of the pointer key, matching the existing `HoistAnalysis` cache pattern.
- IC invalidation policy, storage layout, and sharing semantics are localized in the interpreter module; future changes should not need to touch the parser or evaluator plumbing.
