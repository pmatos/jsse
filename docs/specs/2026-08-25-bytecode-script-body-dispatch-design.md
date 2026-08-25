# Bytecode script-Body dispatch

## Context

Opt-in bytecode dispatch currently begins at `dispatch_body`, which is called
for ordinary function Bodies. Script execution enters `exec_body` directly, so
an otherwise eligible top-level numeric loop never reaches `compile_body`.
Release-mode measurements for issue #525 show no material change for the
top-level loop under `--bytecode`, while wrapping the same loop in a function
produces the expected bytecode speedup.

Scripts cannot be treated exactly like functions. ScriptEvaluation performs
GlobalDeclarationInstantiation before evaluation and returns the Script's
StatementList completion, replacing only an empty result with `undefined` at
the host boundary. Function chunks instead create function-scoped `var`
bindings and return `undefined` on fallthrough. Direct eval has a separate
EvalDeclarationInstantiation path and is not part of this slice.

## Approaches considered

1. Add a script-aware compiler goal and dispatch eligible Script Bodies after
   declaration instantiation. This is selected because it preserves the
   existing compiler/VM seam and offers the complete Body to the same
   all-or-nothing eligibility membrane.
2. Wrap the Script Body in a synthetic function. This is rejected because a
   function environment changes global `var` properties, top-level `this`,
   direct-eval scope, and fallthrough completion semantics.
3. Compile only top-level statements proven not to produce completion values.
   This is rejected because iteration statements propagate the latest
   value-producing body completion. It would either miscompile or exclude the
   motivating `for` loop with an assignment expression in its body.

## Design

`exec_statements_cached` will be separated into two internal phases:

- declaration instantiation performs the existing global declaration checks,
  `var`/function/lexical hoisting, and Annex B setup;
- prepared statement execution performs the existing StatementList loop and
  `UpdateEmpty` behavior.

The tree-walker continues to call both phases in order. Script bytecode
dispatch first attempts compilation. If compilation succeeds, it runs the
same declaration-instantiation phase and then executes the Chunk. If
compilation rejects any syntax, the existing tree-walker path runs unchanged.
Compiling before declaration instantiation is side-effect free; in particular,
an ineligible script does not partially mutate the global environment before
falling back.

The compiler gains a separate `compile_script_body` entry point while retaining
`compile_body` for function semantics. Both use the same expression and
statement lowering. In script mode only, an expression statement emits a
`SetCompletion` instruction instead of `Pop`, and the compiler appends
`ReturnCompletion` instead of `ReturnUndefined`.

The VM keeps an optional current StatementList completion for script chunks.
`SetCompletion` replaces it with the expression's value; empty statements,
declarations, untaken branches, loop tests, loop updates, and variable
initializers leave it unchanged. A single dynamic accumulator is sufficient
for the compiler's supported structured statements: it records the last
value-producing expression statement actually evaluated, which is the result
of the specification's nested `UpdateEmpty` operations. `ReturnCompletion`
returns `Completion::Normal(value)` when populated and `Completion::Empty`
otherwise.

Object-valued completion values are mirrored into `gc_bytecode_roots`, just
like operand-stack entries. Replacing the accumulator unroots the old value and
roots the new value. The existing bytecode frame cleanup releases the retained
root on normal or abrupt exit.

The top-level dispatcher installs the Script Body's IC handle around VM
execution and keeps the global environment in `call_stack_envs`, matching the
tree-walker Body lifetime. Script Bodies are normally evaluated once, so this
slice does not add a second bytecode cache keyed by AST identity; function
object caching remains unchanged.

## Eligibility and scope

The current all-or-nothing eligibility boundary remains authoritative. This
means direct calls named `eval`, top-level `this`, lexical declarations,
function/class declarations, and every other unsupported form fall back to the
tree-walker. Sloppy direct-eval scoping is therefore unchanged.

Only `SourceType::Script` entry points use this dispatch. Dynamic eval keeps
`exec_eval_body`, because it performs EvalDeclarationInstantiation and exposes
eval-specific scope rules. Modules are also excluded: executable module items
live in `Program.module_items`, not `Program.body`, and need a separate
module-item compiler design.

## Validation

- A bytecode unit test executes an eligible numeric `for` loop directly at
  script top level and asserts that exactly one Chunk runs and the global
  result matches the tree-walker.
- Script-completion parity tests cover a final expression, later empty
  statements/declarations, zero- and multi-iteration loops, conditional
  branches, and object-valued completions surviving a forced GC.
- Global-scope tests cover `var` creation as a global property and declaration
  checks before execution. Direct eval remains covered by fallback assertions.
- Targeted test262 runs cover global code, script code, statement completion,
  loops, and eval code under `--bytecode`, followed by the full normal and
  `--bytecode` suites.
- A release-mode timing comparison must show a material improvement for the
  top-level numeric loop under `--bytecode`, comparable in direction to the
  function-wrapped control.

## Success criteria

An eligible top-level numeric `for` loop executes a bytecode Chunk under the
opt-in flag, retains script global-declaration and completion semantics, gains
a measurable release-mode speedup, and introduces no test262 regression in
either execution mode.
