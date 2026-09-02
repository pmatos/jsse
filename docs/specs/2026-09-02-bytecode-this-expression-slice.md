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

1. **Lower `this` through `Op::LoadName`.** Selected. The existing operation
   resolves an environment binding, propagates its abrupt completion, pushes
   the result, and roots object values on the bytecode operand stack. Using the
   interned name `"this"` therefore preserves the engine's existing model with
   no new VM semantics.
2. **Add a dedicated `Op::LoadThis`.** Rejected for this slice. It would
   duplicate name lookup, TDZ handling, stack accounting, GC rooting, opcode
   reporting, and tests without producing different behavior in JSSE's
   environment representation.
3. **Compile `super` and `new.target` in the same change.** Rejected. Although
   they also consult call-environment state, their syntax and runtime rules are
   distinct eligibility slices.

## Lowering and invariants

`Compiler::compile_expr` interns `"this"`, emits `LoadName` with that name
index, and records one pushed operand. Everything downstream remains shared
with identifier loads:

- strict bare calls observe `undefined`;
- sloppy bare calls observe the substituted global object;
- derived constructors throw `ReferenceError` when the binding is read before
  `super()`;
- object-valued `this` bindings use the existing bytecode-root stack;
- an unsupported later node still rejects the whole Body and reports that
  later node as the first bail reason.

No opcode, constant-pool format, IC state, object kind, or GC walker changes.

## Validation

Bytecode parity tests cover strict and sloppy bare-call binding and a derived
constructor whose otherwise eligible `return this` Body must throw before
`super()`. The last case deliberately avoids an explicit `super()` call so it
executes the compiled TDZ path instead of passing through the AST fallback.

Targeted test262 coverage is the `this` expression, function statement, and
class-subclass directories in both default and `--bytecode` modes. The full
default test262 suite remains the regression gate. Counter reports for
TypeScript-Octane and OfflineAssembler are recorded under
`docs/perf/2026-09-02/`.
