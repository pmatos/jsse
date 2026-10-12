# Function-call environment pooling design

## Goal

Reduce the fixed cost of ordinary ECMAScript calls, especially mandreel-style
sloppy functions with one identifier parameter, without changing observable
environment, parameter, closure, or `arguments` semantics.

## Specification constraints

`PrepareForOrdinaryCall` creates a new Function Environment Record for each
activation, and `FunctionDeclarationInstantiation` creates and initializes the
parameter bindings in that record. A simple parameter list contains only
identifier bindings. Sloppy functions with simple parameters receive a mapped
arguments object whose indexed properties remain linked to those bindings.
Escaped closures and suspended generators retain their activation records after
the call stack has moved on.

JSSE can therefore reuse an environment's Rust allocation only when no
observable object retains that activation. Reuse must reset every Environment
field, detach the old parent, and clear all bindings before the environment is
placed on a free list.

## Design

Add a small bounded free list of function-scope `EnvRef`s to `Interpreter`.
Ordinary synchronous calls acquire their function environment from this list,
reset it with the new parent, and reserve enough binding capacity for the known
parameters plus `this` and `arguments`. On return, the environment is recycled
only when `Rc::strong_count` proves that the call owns the sole remaining
reference. A closure, mapped arguments object, nested body environment, or any
other escape adds an owner and automatically prevents recycling. Generators and
async functions keep their existing allocation path because their environments
are intentionally retained for later execution.

For sloppy functions whose syntax cannot reference `arguments`, retain the
Annex-B-compatible `Function.prototype.arguments` behavior without eagerly
building an arguments object. The active `CallFrame` stores the original first
argument inline and any remaining arguments in a vector, plus the function
environment. The getter materializes the mapped object on first access, caches
it in the frame for stable identity, and returns it. The GC root walk traces
deferred argument values. Zero- and one-argument calls perform no heap
allocation for this payload.

For a cached simple parameter list, bind parameters directly into the function
environment's map as initialized mutable bindings. Non-simple parameters retain
the existing `bind_pattern` path, including rest arrays, destructuring, default
expressions, and abrupt completion handling. All function kinds use the shared
binding helper, while only ordinary synchronous calls participate in pooling.

The pool is bounded to avoid retaining storage proportional to pathological
recursion depth. Environments are cleared before pooling so the free list never
acts as a GC root for values or outer environments.

## Fixed per-call work (#899)

Later changes removed work every call paid regardless of whether it was
observable:

- **Call-path snapshot.** `call_function_inner_impl` copies only the fields the
  call reads out of the callee (`CallTarget`) instead of cloning the whole
  `JsFunction`, whose `name` and `source_text` allocated per call.
- **`this`.** Sloppy undefined/null `this` resolves through
  `Realm.global_object` rather than a global-environment lookup, and
  `last_call_this_value` — read only by `[[Construct]]` — is written only for
  constructor calls.
- **`arguments`.** A function that never mentions `arguments` (no reference,
  declaration, parameter or direct eval, arrow parameters included) gets no
  placeholder binding; its sloppy frame keeps the arguments inline
  (`SmallVec`) for the legacy `fn.arguments` accessor. Arguments objects are
  built by direct property insertion, and `@@iterator` is the realm's
  `%Array.prototype.values%` (`Realm.array_prototype_values`, a GC root).
- **VM.** `Op::Call`/`Op::Construct` hand the callee a slice of the operand
  stack (the operands stay rooted and are popped after the call), chunk
  activations recycle their operand/reference stacks through
  `Interpreter.vm_stack_pool`/`vm_ref_pool`, and `IcStore` keys bodies with
  FxHash.
- **Non-simple parameters.** The separate var environment's name set comes from
  the per-Body hoist cache instead of a body walk and two `HashSet`s per call.

What remains per call is the `String`-keyed `Environment.bindings` insert for
`this` and each parameter, which belongs to the slot-keyed environment work
(#898).

## Alternatives considered

Direct binding and pre-sizing alone have the smallest change surface, but leave
the `Rc<RefCell<Environment>>` and sloppy arguments allocations on every hot
call. Pooling with the existing eager arguments object is safe but ineffective
for the motivating sloppy-function workload because the mapped arguments
object retains the environment. Static escape analysis could avoid the runtime
strong-count check, but correctly classifying closures, direct eval, and
indirect observation would add substantially more complexity than the bounded
dynamic ownership check.

## Verification

- Add focused custom coverage for simple and duplicate parameter binding,
  closure escape across subsequent calls, mapped `arguments`, and lazy
  `Function.prototype.arguments` identity and aliasing.
- Run the test262 function-code and arguments-object areas, then the full suite
  against the `origin/main` pass baseline.
- Compare repeated release-mode timings of a one-parameter sloppy function call
  loop before and after the change.
- Run formatting, Clippy, release build, and release tests before publishing.
