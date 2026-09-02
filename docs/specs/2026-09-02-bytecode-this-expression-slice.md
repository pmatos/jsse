# Bytecode `this` expression slice

## Context

Issue #524 found that the bytecode compiler rejects most real-world Bodies at
the first unsupported AST node. The bail-reason counters added in #537 show
that `expression:This` is the highest-ranked self-contained expression blocker
across the measured workloads. Method-call callees occur more often, but need
the Body-local IC plumbing deferred from #398. This slice makes `this`
eligible without widening that separate seam.

## Specification basis

The pinned ECMAScript specification defines evaluation of
`PrimaryExpression : this` in `sec-this-keyword` as
`ResolveThisBinding()`. `sec-getthisenvironment` walks the lexical environment
chain to the Environment Record that supplies a `this` binding, and
`sec-resolvethisbinding` returns that record's `GetThisBinding()` result.
Function Environment Records (`sec-function-environment-records`) also make
the derived-constructor case abrupt: the binding remains uninitialized until
`super()` initializes it.

JSSE already represents `this` as an environment binding. Call setup installs
the strict, sloppy-substituted, or uninitialized value before `dispatch_body`,
and `resolve_identifier` performs the existing TDZ check. The tree-walker's
dedicated `Expression::This` arm uses the same model.

## Approaches considered

1. **Lower `this` through `Op::LoadName`.** Initially selected, then reverted
   (see "Post-merge correction" below): `LoadName` runs ordinary identifier
   resolution, which consults `with` object environments before falling back
   to lexical bindings. `this` must use `ResolveThisBinding`
   (`sec-resolvethisbinding`), which skips object Environment Records
   entirely. Reusing `LoadName` therefore let a `with` object shadow `this`
   with an ordinary property (even invoking a getter) whenever a nested,
   independently-compiled function body closed over the with-environment.
2. **Add a dedicated `Op::LoadThis`.** Originally rejected as duplicating name
   lookup, TDZ handling, stack accounting, GC rooting, opcode reporting, and
   tests "without producing different behavior in JSSE's environment
   representation" — disproven by the `with`-shadowing case above, and now the
   shipped design (see "Post-merge correction").
3. **Compile `super` and `new.target` in the same change.** Rejected. Although
   they also consult call-environment state, their syntax and runtime rules are
   distinct eligibility slices.

## Post-merge correction

Review on the landing PR (pmatos/jsse#578) found that `Op::LoadName` observably
diverges from `Expression::This`'s tree-walker semantics: `with ({ this: 42 })
{ f = () => this; }` returned `42` under `--bytecode` instead of the enclosing
`this`, because `resolve_identifier` — `LoadName`'s resolver — checks
`with_object` at each scope before falling back to bindings, while
`Environment::get` (`types.rs`), which the tree-walker uses, only ever walks
`bindings`. The fix adds a dedicated `Op::LoadThis` backed by a new
`Interpreter::resolve_this_binding` helper — extracted from the tree-walker's
`Expression::This` arm so both interpreters share one implementation of
`ResolveThisBinding` rather than two independently-maintained copies. This
also fixes a second, narrower divergence: `LoadName`'s global-object/
`ReferenceError` fallback path does not apply to `this` at all.

## Lowering and invariants

`Compiler::compile_expr` emits `Op::LoadThis` — a zero-operand op, since no
name lookup is involved — and records one pushed operand. The VM dispatches it
through `Interpreter::resolve_this_binding`, the same helper the tree-walker's
`Expression::This` arm calls, so both paths stay in lockstep by construction:

- strict bare calls observe `undefined`;
- sloppy bare calls observe the substituted global object;
- derived constructors throw `ReferenceError` when the binding is read before
  `super()`;
- `with` objects are never consulted, even when they expose a `this` property;
- object-valued `this` bindings use the existing bytecode-root stack;
- an unsupported later node still rejects the whole Body and reports that
  later node as the first bail reason.

One opcode (`Op::LoadThis`) was added; constant-pool format, IC state, object
kind, and GC walker are unchanged.

## Validation

Bytecode parity tests cover strict and sloppy bare-call binding, a derived
constructor whose otherwise eligible `return this` Body must throw before
`super()`, and a nested arrow whose bytecode-eligible body closes over a
`with` object exposing a `this` property — the case that caught the
`Op::LoadName` divergence. The derived-constructor case deliberately avoids an
explicit `super()` call so it executes the compiled TDZ path instead of
passing through the AST fallback.

Targeted test262 coverage is the `this` expression, function statement, and
class-subclass directories in both default and `--bytecode` modes. The full
default test262 suite remains the regression gate. Counter reports for
TypeScript-Octane and OfflineAssembler are recorded under
`docs/perf/2026-09-02/`.
