# Global environment and the global object (#1346)

The Global Environment Record is two records (sec-global-environment-records):
a declarative record for `let`/`const`/`class`, and an Object Environment Record
whose binding object is the global object. jsse follows that split exactly.

## Invariants

- `Environment.bindings` of the global environment holds only the declarative
  record (`Let`, `Const`, and the immutable `undefined`/`NaN`/`Infinity`).
  `var` names, function declarations and implicit globals are **own properties
  of the global object and nothing else**. There is no second, mirrored `Var`
  binding that could go stale, so a property redefined as an accessor, deleted,
  or replaced through `globalThis`, `Reflect` or `Object.defineProperty` is what
  a bare name sees. `global_environment_keeps_no_var_bindings`
  (`src/interpreter/global_access/record/tests.rs`) pins this.
- Names the engine installs itself (`Test262Error`, `$DONOTEVALUATE`, `print`,
  `globalThis`, `$262`) end up on the global object too:
  `builtins/global_object.rs` strips their bootstrap bindings
  (`BUILTIN_GLOBAL_NAMES`) once the global object exists.
- Everything that touches the object record goes through the global object's
  internal methods, so a Proxy (or any exotic) global object sees it:
  `src/interpreter/global_access/record.rs` implements `HasRestrictedGlobalProperty`,
  `CanDeclareGlobalVar`, `CanDeclareGlobalFunction`, `CreateGlobalVarBinding`,
  `CreateGlobalFunctionBinding`, `DeleteBinding` and the sloppy
  `Set(globalObject, name, value, false)` behind implicit globals and var
  initializers (`env_set` of a name no declarative scope binds).
  GlobalDeclarationInstantiation (`instantiate_body_declarations`) and
  `EvalDeclarationInstantiation` call them; a trap that throws propagates out of
  declaration instantiation.
- The Global Data Fast Path (`global_access.rs`) is unchanged: it only handles an
  ordinary global object's own, writable data property and refuses accessors,
  non-writable properties, inherited names and Proxy globals, which take the
  generic `[[HasProperty]]`/`[[Get]]`/`[[Set]]` route. With no `Var` binding in
  the declarative record the scope walk now ends at `NameWalk::Global` for every
  global `var` and function name, so the fast path serves them.

## Testing a Proxy global

JavaScript cannot make a realm's global object a Proxy. The unit tests in
`src/interpreter/global_access/record/tests.rs` create a realm with
`$262.createRealm()` and replace its `global_object` / `global_env.global_object_id`
with a logging Proxy over the original global. The accessor, delete, `var`,
function and implicit-global cases for a plain global object are test262-extra
files (`global-implicit-*`, `global-eval-function-*`, `global-var-initializer-*`,
`global-function-declaration-*`, `global-annexb-function-*`).

## Follow-up

`node-lib/internal/vm/sync.js` still mirrors a contextified sandbox onto an
ordinary global object. Replacing it with a Proxy global over the sandbox is now
possible (it needs a host hook to create a realm around a given global object)
and would make sandbox properties live; it is not part of #1346.
