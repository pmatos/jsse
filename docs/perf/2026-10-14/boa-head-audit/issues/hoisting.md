Prettier's Web Tooling Benchmark passes on Boa HEAD and Node but rejects on JSSE with `ReferenceError: s is not defined`. The missing `s` is a function declaration at the end of Prettier's async `Ye` function. Minimization shows that no Prettier dependency, await expression, or complex formatting input is needed.

## Minimal reproducer

```js
async function f() {
  return s;
  function s() {}
}
f().then(
  value => console.log(typeof value),
  error => console.log(error.name + ":" + error.message)
);
```

Expected on Node and Boa: `function`. Actual on JSSE: `ReferenceError:s is not defined`.

The same result occurs with default bytecode and `--no-bytecode`, in fresh processes at JSSE `2ea6417535037ca58d78a433a72a2a0e2dfb4d77`. Boa is `f0789a20d87ad52eef6460953929c2b3cdbf8716`; Node is v26.8.2. The rejection is explicitly handled above so the verdict does not depend on host unhandled-rejection policy.

## Related generator reproducer

```js
function* g() {
  yield s();
  function s() { return 42; }
}
console.log(g().next().value);
```

Node and Boa print `42`. Both JSSE modes exit 1 with `ReferenceError: s is not defined`. An async generator returning the later-declared function without any yield also rejects. Moving the declaration before its use makes JSSE pass; the corresponding ordinary synchronous function passes. A synchronous generator that reads the declaration in the same post-yield fragment also passes, which isolates visibility across lowered fragments rather than a general parser failure.

## Specification

[FunctionDeclarationInstantiation](https://tc39.es/ecma262/#sec-functiondeclarationinstantiation) derives functions to initialize from the complete body's VarScopedDeclarations, resolves duplicate declaration names, creates the function objects and installs their bindings before body execution. Unreachable placement after a return does not remove the declaration. Both [EvaluateAsyncFunctionBody](https://tc39.es/ecma262/#sec-runtime-semantics-evaluateasyncfunctionbody) and [EvaluateAsyncGeneratorBody](https://tc39.es/ecma262/#sec-runtime-semantics-evaluateasyncgeneratorbody) require that instantiation step.

## Potential root cause, with source evidence

- [`call_async_function`](https://github.com/pmatos/jsse/blob/2ea6417535037ca58d78a433a72a2a0e2dfb4d77/src/interpreter/eval.rs#L7957) declares state-machine temporaries/local variables and runs the original body's Annex B registration, but does not instantiate all original function-level declarations.
- [`transform_generator_inner_opts`](https://github.com/pmatos/jsse/blob/2ea6417535037ca58d78a433a72a2a0e2dfb4d77/src/interpreter/generator_transform.rs#L639) lowers async bodies with returns even if there is no await. [`Statement::Return` lowering](https://github.com/pmatos/jsse/blob/2ea6417535037ca58d78a433a72a2a0e2dfb4d77/src/interpreter/generator_transform.rs#L1210) advances to an unreachable state, leaving trailing function declarations outside the executing fragment.
- [`exec_state_machine_body`](https://github.com/pmatos/jsse/blob/2ea6417535037ca58d78a433a72a2a0e2dfb4d77/src/interpreter/exec.rs#L27) executes/instantiates only each fragment. [`hoist_annexb_at_state_machine_entry`](https://github.com/pmatos/jsse/blob/2ea6417535037ca58d78a433a72a2a0e2dfb4d77/src/interpreter/exec.rs#L415) registers Annex B names; it is not the full function-declaration instantiation required here.

A temporary diagnostic build associated active machines with their original function source. At the failure, Prettier's `Ye` machine had local variables e/t/r/n/u/o/i and was reading `s`, while its original source declared `function s` and `function a` after its return. The measured release snapshots and main source checkout were unchanged; the diagnostic build was not used for timing.

## Suggested fix boundary and validation

Instantiate original function-level declarations once at function/generator entry, independently of fragment reachability. Preserve block-scoped/Annex B instantiation, default-parameter/body environment separation, duplicate-declaration ordering and closure identity. Simply pre-running arbitrary original statements or repeatedly recreating declarations per state would change semantics.

Add focused test262-extra cases for async functions, async generators and generators with declarations after return/yield/await; include declaration identity across a suspension, duplicate names (last declaration wins), captures, strict bodies, and non-simple parameters. Relevant existing directories are `language/statements/async-function`, `language/statements/async-generator`, `language/statements/generators`, and their expression counterparts. Rerun Prettier's full WTB workload and the required full test262/GC-stress gates for a fix.

Raw cross-engine output, six reduction controls, two generator controls and the minimized fixture are retained under `docs/perf/2026-10-14/boa-head-audit/issues/` locally. No engine fix was made during this audit.
