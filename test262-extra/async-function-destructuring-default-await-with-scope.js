/*---
description: >
  A destructuring default that awaits inside a with statement still resolves
  identifiers through the with object after the function resumes.
esid: sec-runtime-semantics-keyedbindinginitialization
info: |
  KeyedBindingInitialization : BindingElement : BindingPattern Initializer_opt

  ...
  3. If Initializer is present and v is undefined, then
    a. Let defaultValue be ? Evaluation of Initializer.
  ...
flags: [async, noStrict]
includes: [compareArray.js]
features: [async-functions, destructuring-binding]
---*/

async function f(L) {
  var o = { q: 1 };
  with (o) {
    var { a = await 1 } = {};
    L('a' + a + q);
  }
}

var log = [];
var L = function (x) { log.push(x); };
var ticks = Promise.resolve().then(function () { L('w1'); }).then(function () { L('w2'); }).then(function () { L('w3'); });
var p = f(L);
L('sync-end');
Promise.all([p, ticks]).then(function () {
  assert.compareArray(log, ['sync-end', 'w1', 'a11', 'w2', 'w3']);
}).then($DONE, $DONE);
