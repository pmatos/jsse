# Bytecode numeric-loop slice

## Context

Issue #67 already has an opt-in, function-body stack VM with a conservative
eligibility membrane. It compiles literals, identifier reads and simple
assignment, arithmetic and comparison expressions, conditionals, and `if`
statements. It cannot compile variable declarations, update expressions, or
loops, so the tight numeric loop that motivated the issue still falls back to
the tree-walker.

This slice makes that workload eligible without changing the default execution
mode or weakening the whole-body fallback.

## Approaches considered

1. Extend the existing stack VM vertically. Add only the bindings, expressions,
   and backward control flow needed for numeric `for` and `while` loops. This is
   the selected approach because it reuses the reviewed compiler/VM seam and
   can be validated against the tree-walker.
2. Introduce indexed local slots before adding loops. This should ultimately
   outperform name-based environment lookup, but it requires a new Environment
   interface and compile-time binding analysis. It is a larger semantic change
   and is deferred until loop dispatch itself is measurable.
3. Compile every statement form and enable bytecode by default. This would make
   issue #67 look complete sooner, but calls, lexical environments, abrupt
   completions, and `try`/`finally` need substantially more VM machinery. The
   risk is disproportionate to this slice.

## Design

The bytecode module remains a deep module with one external seam:
`compile_body` either returns a complete `Chunk` or rejects the entire Body.
`dispatch_body` continues to cache that outcome and selects either the VM or the
tree-walker. There is no instruction-level fallback.

The compiler records function-scoped `var` names as indices into the Chunk's
existing name pool. Before dispatch, the VM creates any missing mutable `var`
bindings with `undefined`. This follows FunctionDeclarationInstantiation: the
binding exists before any initializer is evaluated, while each initializer is
executed in source order. Only identifier patterns are eligible; destructuring
and lexical declarations remain on the tree-walker.

Simple identifier update expressions use one `UpdateName` instruction carrying
the name, increment/decrement operation, and prefix/postfix mode. Its handler
uses the interpreter's existing identifier-update semantics, preserving
ToNumeric, BigInt, strict-mode, TDZ, const assignment, and returned-value rules.
Non-logical compound assignments to identifiers lower to the existing
load/binary-operation/store sequence. Logical assignments remain unsupported.

`while` lowers to a condition header, a false exit, the body, and a backward
jump. `for` lowers initializer, condition, false exit, body, update, and a
backward jump in specification order. An absent condition emits no exit branch.
Every backward jump performs the same GC safepoint that the tree-walker performs
once per iteration. The compiler requires the operand stack to be empty at the
backedge.

This slice accepts expression initializers and `var` initializers. `let` and
`const` loop heads remain ineligible because CreatePerIterationEnvironment is
observable through closures. `break`, `continue`, labels, `do`/`while`, calls,
member access, and other unsupported nodes also reject the entire Body.

## Error handling and limits

Backward and forward jumps continue to use signed 16-bit relative offsets. If
a loop body does not fit, compilation returns `CompileError::Unsupported` and
the Body is permanently cached as ineligible. Runtime JavaScript abrupt
completions from name lookup, update, coercion, or assignment immediately leave
the VM unchanged.

## Validation

- Compiler/VM tests cover var hoisting before initializers, multiple var
  declarators, prefix/postfix increment/decrement (including BigInt through an
  argument), `while`, `for`, absent loop clauses, and unsupported lexical loop
  fallback.
- End-to-end parity tests run each eligible function in tree-walker and bytecode
  modes and assert that a Chunk actually executed.
- A release-mode numeric-loop timing compares the two modes as performance
  evidence, without making wall-clock timing a unit-test assertion.
- Targeted test262 runs cover variable statements, update expressions, and
  `for`/`while`, followed by the full suite in normal mode and the required Rust
  quality gate.

## Success criteria

The function equivalent of the motivating loop,
`for (var i = 0; i < n; i++) sum += i`, executes a bytecode Chunk, returns the
same result as the tree-walker, retains a per-iteration GC safepoint, and does
not introduce a test262 baseline regression.
