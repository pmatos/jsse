# Per-node tree-walker fallback inside compiled function bodies

Status: accepted (#902)

## Context

Bytecode eligibility was all-or-nothing per function body. One construct the
compiler could not lower (`switch`, `throw`, `typeof`, `let`/`const`,
array/object literals, member `++`/`--`, `instanceof`, `in`, BigInt literals,
`try`, `for-in`/`for-of`, function expressions) sent the *entire* body to the
tree-walker, ~1.8x slower than the VM. The `bench_numeric_bail_*` repros add a
single never-executed statement to a hot numeric loop and lose that factor;
the JetStream bail tables put 60-99% of tree-walker work behind such guard
clauses (AES `throw`, gbemu `switch`, crypto/richards member `++`).

## Decision

1. **Two hand-off ops.** `Op::EvalAst idx` evaluates an expression node with
   `eval_expr` in the frame's environment and pushes the result (operands
   already pending stay rooted in `gc_bytecode_roots`). `Op::ExecAst idx
   table` runs a statement node with `exec_statement` at a statement boundary.
   Nodes are deep clones held in `Chunk.ast`; nested function `Body`s remain
   `Rc`-shared, so body identity (bytecode memo, hoist cache, IC stores) is
   unchanged. Function goal only: the Script goal keeps bailing whole (superseded by ADR-2026-10-13-1210, which extends the fallback to it).
2. **Transactional wrappers.** `compile_expr` / `compile_statement` snapshot
   compiler state, and on `CompileError::Unsupported` roll back and emit one
   hand-off instead. `Super`, private names and `Spread` are not standalone
   expressions, so their error propagates to the parent node.
3. **`Ineligible` is whole-body.** Direct `eval`, `with`, `super`,
   `new.target`, `import.meta`/`import()`, `yield`/`await`, private `in`, and
   the resource limits keep the body on the tree-walker, as before. A one-time
   pre-scan (`body_has_poison`) finds them before anything is emitted, because
   once a parent falls back its subtree is never visited. A body whose only
   content would be hand-offs is also `Ineligible("fallback-only body")`: it
   would pay chunk-entry cost for nothing.
4. **Parent pre-checks run before children compile**, because an inner
   wrapper succeeds by emitting `EvalAst` and the parent would never see an
   error:
   - *Name inference*: assignment / declarator whose value
     `is_anonymous_function_definition()` falls back at that node, since
     NamedEvaluation is applied by the parent.
   - *Scope rule* (superseded by ADR-2026-10-13-0632, which gives the VM
     block environments; only `using` blocks and a `switch` whose clauses
     declare functions still take this path): the VM has no block
     environments, so a block, `switch` or loop head that declares a binding
     (`let`/`const`/`using`, class, function) runs as one `ExecAst` unit and
     the tree-walker creates the scope, hoists TDZ and makes per-iteration
     copies. Function top level is exempt: its lexicals live in the function
     environment.
   - *Tail position*: `return <expr containing a call>` (other than the direct
     `return f(...)` lowering) falls back at the `return`, so `TailCall` is
     produced by the tree-walker; `TaggedTemplate` counts as a call.
5. **Completion routing.** `exec_statement` completions `Return`, `TailCall`,
   `Throw` and `Exit` leave the chunk unchanged. `Break`/`Continue` look up
   the statement's escape table (entries for every enclosing compiled loop /
   `switch` frame and its labels, targets patched when the frame closes); a
   miss returns the completion unchanged.
6. **Hoisting without a per-call tax.** `var` names found inside fallback
   statements join `Chunk.var_names` (declared by the existing prologue).
   Declaration instantiation (`instantiate_body_declarations`, with the cached
   hoist analysis) runs before the chunk only when `Chunk.needs_instantiation`:
   the body declares a function anywhere (Annex B) or a top-level lexical
   binding. The chunk's var prologue runs first so slot hints keep their
   expected binding order. Top-level function declarations compile to nothing.
7. **Native lowerings** for the constructs with the best measured payoff:
   `throw`, `typeof` (`TypeofName` for unresolvable-safe identifiers),
   member `++`/`--` (`UpdateProp`/`UpdateElement`, base and key evaluated once),
   `instanceof`, `in`, BigInt literals (a constant), `switch` (selectors
   tested lazily with `StrictEq`, trampolines, `switch` frames are break but
   not continue targets), and function-top-level `let`/`const`
   (`InitLexical` initialises the TDZ binding instantiation hoisted).
8. **Observability.** The counters build reports `FALLBACK` rows (static
   hand-off nodes per reason), `ast_fallback_execs` and
   `ast_units_in_fallbacks`; fallback work is attributed exclusively to the
   enclosing function's `BODY` row without counting as an AST-body
   invocation, so "BAIL -> 0" cannot be read as "no tree-walker work".

## Consequences

- One unsupported node costs only that node. A `bench_numeric_bail_*` repro
  sits within 1.1x of `bench_numeric_bail_none`.
- Constructs without a native lowering that execute hot (`try`, `for-in`/`for-of`,
  array/object literals, closures, block-scoped `let`/`const`) still run on
  the tree-walker at statement/expression granularity; `ast_fallback_execs`
  and the `FALLBACK` rows say where.
- A jump-offset overflow inside one statement falls back for that statement
  only instead of making the body ineligible.
- The tree-walker's primitive-base member update no longer evaluates the base
  and key twice (found while writing the native op).
