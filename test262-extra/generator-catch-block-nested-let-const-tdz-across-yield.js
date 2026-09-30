/*---
description: >
  A catch clause's body is textually a Block per grammar
  (Catch : catch ( CatchParameter ) Block), so it gets its own
  BlockDeclarationInstantiation, distinct from (and nested inside) the
  catch parameter's own environment. A shadowing let/const declared in
  the catch body -- for a name other than the catch parameter itself --
  with a suspension point (yield) anywhere in the body, must keep the
  shadowed name in TDZ from block entry, even when the generator's
  state-machine lowering splits the catch body's StatementList across the
  yield.
esid: sec-runtime-semantics-catchclauseevaluation
info: |
  CatchClauseEvaluation

  1. Let oldEnv be the running execution context's LexicalEnvironment.
  2. Let catchEnv be NewDeclarativeEnvironment(oldEnv).
  3. For each element argName of the BoundNames of CatchParameter, do
    a. Perform ! catchEnv.CreateMutableBinding(argName, false).
  ...
  6. Let status be Completion(BindingInitialization of CatchParameter ...).
  ...
  9. Return ? Evaluation of Block.

  Block Evaluation (sec-block-runtime-semantics-evaluation) then creates a
  second, nested Environment Record for the catch body's own lexical
  declarations, all entering TDZ together before any statement of the
  body runs -- independent of how the generator's state-machine lowering
  splits it across a suspension point.
features: [generators]
---*/

var outerX = 'outer';
function* g() {
  var x = outerX;
  try {
    throw 1;
  } catch (e) {
    const y = (function () { return x; })();
    yield;
    const x = 1;
    return y;
  }
}

var it = g();
assert.throws(
  ReferenceError,
  function () { it.next(); },
  'reading the shadowed name before its own declaration runs in the catch body throws a TDZ ReferenceError'
);

function* positive() {
  var a = 'outer';
  try {
    throw 1;
  } catch (e) {
    let a = 1;
    yield;
    return a;
  }
}
var it2 = positive();
it2.next();
assert.sameValue(
  it2.next().value,
  1,
  'a let declared before the suspension point in the catch body keeps its own value after resuming'
);
