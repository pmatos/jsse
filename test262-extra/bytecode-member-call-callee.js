// Spec: ECMAScript 2025, sec-function-calls-runtime-semantics-evaluation
//       (EvaluateCall with a property-reference callee; thisValue = base)
// A call whose callee is a member expression must evaluate the base, then the
// key, then GetValue, then the arguments, and call with the base as `this`.

function floorSum(o) {
  var s = 0;
  for (var i = 0; i < 4; i++) { s += Math.floor(o[i] / 2); }
  return s;
}
assert.sameValue(floorSum([1, 2, 3, 5]), 4, 'Math.floor member call in a loop');

var obj = { v: 7, m: function (a) { return this.v + a; } };
function viaObj() { return obj.m(1) + obj['m'](2); }
assert.sameValue(viaObj(), 17, 'receiver is the base object');

function viaPrimitive() { return 'abc'.charCodeAt(1) + (5).toFixed(1).length; }
assert.sameValue(viaPrimitive(), 101, 'primitive base resolves through its prototype');

var order = [];
var withGetter = { get m() { order.push('get'); return function () { order.push('call'); }; } };
function ordered() {
  withGetter[(order.push('key'), 'm')]((order.push('arg'), 1));
  return order.join();
}
assert.sameValue(ordered(), 'key,get,arg,call', 'evaluation order');

function nullishBase() { var n = null; n.x(); }
assert.throws(TypeError, nullishBase, 'nullish base throws TypeError');

function missingMethod() { var o = {}; o.nope(); }
assert.throws(TypeError, missingMethod, 'undefined callee throws TypeError');
