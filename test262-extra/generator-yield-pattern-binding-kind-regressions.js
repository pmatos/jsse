/*---
description: >
  Routing a yield/yield*-suspended pattern destructuring through the
  declaration's real BindingKind (var/let/const) must preserve every
  existing semantic of that kind: const stays immutable, a genuine TDZ
  self-reference still throws, a shadowed outer binding is never read in
  its place, and var keeps its function-wide scope.
esid: sec-let-and-const-declarations-runtime-semantics-evaluation
info: |
  LexicalBinding : BindingPattern Initializer

  BindingInitialization for a BindingPattern goes through
  InitializeReferencedBinding for let/const (so a later assignment is a
  PutValue against an immutable binding, and any name the pattern's own
  Initializer reaches before its declaration is still in the TDZ) and
  through PutValue for var (so the declaration's scope is unaffected by
  how its initializer was evaluated).
features: [generators, destructuring-binding]
---*/

// const stays immutable after a suspended pattern initializes it.
function* constStaysConst() {
  const { x } = yield;
  x = 5;
}
var it = constStaysConst();
it.next();
var thrown;
try {
  it.next({ x: 1 });
  thrown = 'nothing thrown';
} catch (e) {
  thrown = e;
}
assert.sameValue(thrown.constructor, TypeError, 'assigning a const bound via a suspended pattern throws TypeError');

// A genuine TDZ self-reference inside the pattern's own default still throws.
function* selfReferenceTdz() {
  const { x = x } = yield;
  return x;
}
it = selfReferenceTdz();
it.next();
thrown = undefined;
try {
  it.next({});
  thrown = 'nothing thrown';
} catch (e) {
  thrown = e;
}
assert.sameValue(thrown.constructor, ReferenceError, 'a default that reaches its own still-TDZ binding still throws');

// A name in the pattern that shadows an outer binding must never resolve to
// the outer binding. `x` is hoisted (uninitialized) into the generator's own
// environment before any statement runs, so even the yield operand --
// evaluated before the first suspension -- resolves the closure's `x` to
// that still-TDZ binding, not to the outer `var`.
var x = 'outer';
function* shadowedOuterBinding() {
  const { x } = yield (function () {
    return x;
  })();
  return x;
}
it = shadowedOuterBinding();
thrown = undefined;
try {
  it.next();
  thrown = 'nothing thrown';
} catch (e) {
  thrown = e;
}
assert.sameValue(
  thrown.constructor,
  ReferenceError,
  'the yield operand reads the shadowing const, still in its TDZ, not the outer var'
);

// var destructured from a suspended value keeps its function-wide scope.
function* varStaysFunctionScoped() {
  if (true) {
    var { y } = yield;
  }
  return y;
}
it = varStaysFunctionScoped();
it.next();
assert.sameValue(it.next({ y: 7 }).value, 7, 'var pattern bound via a suspended value is visible after the block');

// Array patterns go through the same path as object patterns.
function* arrayPattern() {
  const [a, b] = yield* (function* () {
    return [1, 2];
  })();
  return a + b;
}
it = arrayPattern();
assert.sameValue(it.next().value, 3, 'array pattern destructuring a yield* result initializes both bindings');
