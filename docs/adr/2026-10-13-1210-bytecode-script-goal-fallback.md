# Per-node tree-walker fallback and valued completions in the Script goal

Status: accepted (#961)

## Context

ADR-2026-10-12-2214 let a Function-goal body hand the nodes it cannot lower to
the tree-walker (`EvalAst`/`ExecAst`). The Script goal kept bailing the whole
body on the first unsupported node, and almost every script has one: a
function declaration (`statement:FunctionDeclaration`), a global `let`/`const`
or a class. All 45 runnable JetStream drivers reported a `BAIL` on their
`<script body>` row (`lexical declaration`, the function-declaration
equivalent), so their top-level code, including hot top-level loops, ran on
the tree-walker.

## Decision

1. **Per-node fallback in both goals.** `compile_expr` and `compile_statement`
   take the rollback-and-hand-off path regardless of goal. The Script goal
   has no frame locals, so there are no slot hints to keep consistent.
2. **Valued completions.** A Script's completion value lives in the VM's
   accumulator (`SetCompletion`). A statement handed off in Script goal
   carries the `EXEC_SET_COMPLETION` flag; when it completes
   - `Normal(v)`: `v` replaces the accumulator (rooted in `gc_bytecode_roots`),
   - `Empty`: the accumulator is untouched,
   - `Break(label, Some(v))` / `Continue(label, Some(v))`: `v` replaces the
     accumulator before the jump (UpdateEmpty, sec-updateempty), then the
     escape table routes the jump as in the Function goal.

   The compiled `if`, `while`, `do-while` and `for` keep resetting the
   accumulator to `undefined` on entry exactly as before; `switch`, `try`,
   `for-in`, `for-of` and labelled statements are still hand-offs in Script
   goal, so the tree-walker computes their values.
3. **Hoisted functions.** Top-level function declarations emit nothing
   (`compile_statements(.., skip_hoisted = true, ..)`):
   GlobalDeclarationInstantiation already instantiated them before the chunk
   runs, and their statement completion is empty. Global `let`/`const`/class
   declarations stay tree-walker units, because their bindings are global
   declarative records, not the VM's block environments.
4. **Block scopes.** The Script goal no longer refuses `let`/`const` inside a
   block or a `for (let ...)` head: `PushEnv` and `CopyIterEnv` are
   goal-independent, and only the Script top level (which passes
   `owns_lexicals = false`) hands declarations off.
5. **Poison.** The Script goal keeps direct `eval`, `with`, `import()` and
   `import.meta` whole-body, but ignores `await`, `yield`, `new.target`,
   `super` and private `in`: they cannot occur in a Script's own code, only in
   nested arrows, which own them. The JetStream async harness is
   `(async () => { ... await ... })()`, which the shared traversal enters
   because arrows share `new.target`/`super` with their parent.
6. **Fallback-only scripts stay whole.** As for a function body, a Script
   with no native operation at all (only hand-offs) is `Ineligible("fallback-only
   body")`: it would pay a deep clone of every hand-off statement and a chunk
   for nothing.
7. **Observability.** The counters build records a compiled script's
   `FALLBACK` reasons and attributes the tree-walker work its hand-offs spend
   to the `<script body>` row.

## Consequences

- `<script body>` reports `BAIL` only when it holds direct `eval`, `with` or
  `import()`/`import.meta`, or is nothing but hand-offs.
- A hot top-level loop is VM-resident even when the script declares
  functions, classes or global lexicals around it.
- Every test262 script now runs through the VM with per-node hand-offs, which
  exercised two latent bugs fixed alongside: a property read on a primitive
  base passed a transient wrapper object as the getter's receiver
  (`member_get` now uses `access_property_on_value`, which keeps the
  primitive), and `FALLBACK`/`BODY` rows did not cover scripts.
- `switch`, `try`, `for-in`, `for-of` and labelled statements remain
  tree-walker units at Script top level; lowering them natively needs the
  same completion bookkeeping per construct and is left for a follow-up.
