// Copyright (C) 2026 the JSSE authors. All rights reserved.
/*---
esid: sec-async-arrow-function-definitions
description: an async arrow function nested in a formal-parameter default has its own [+Await] context
info: |
  AsyncArrowFunction[In, Yield, Await] :
    CoverCallExpressionAndAsyncArrowHead[?Yield, ?Await] [no LineTerminator here] => AsyncConciseBody[?In]

  AsyncConciseBody[In] :
    [lookahead != {] ExpressionBody[?In, +Await]
    { AsyncFunctionBody }

  An AwaitExpression in the body of the arrow is therefore not an AwaitExpression
  of the enclosing function's FormalParameters. 15.8.1 only forbids the latter
  (the async function's own parameters, and an async arrow's own parameters).
flags: [async]
---*/

function ordinary(a = async () => await 1) { return a; }
function ordinaryBare(a = async x => await x) { return a; }
function ordinaryBlock(a = async (x) => { return await x; }) { return a; }
function* generator(a = async () => await 2) { yield a; }
async function asyncFn(a = async () => await 3) { return a; }
async function* asyncGen(a = async () => await 4) { yield a; }
var expr = function(a = async () => await 5) { return a; };
var arrow = (a = async () => await 6) => a;
var obj = { m(a = async () => await 7) { return a; } };
class C { m(a = async () => await 8) { return a; } }
var nested = (a = async () => (await 9) + (await 10)) => a;

assert.sameValue(typeof ordinary(), "function");

Promise.all([
  ordinary()(),
  ordinaryBare()(11),
  ordinaryBlock()(12),
  generator().next().value(),
  asyncFn().then(function(f) { return f(); }),
  asyncGen().next().then(function(r) { return r.value(); }),
  expr()(),
  arrow()(),
  obj.m()(),
  new C().m()(),
  nested()()
]).then(function(values) {
  assert.sameValue(values.join(), "1,11,12,2,3,4,5,6,7,8,19");
}).then($DONE, $DONE);

var rejected = [
  "async function f(a = await 1) {}",
  "async function f(a = async () => {}, b = await 1) {}",
  "(async function(a = await 1) {})",
  "async (a = await 1) => {}",
  "function f(a = async (b = await 1) => {}) {}",
  "async function f(a = (b = await 1) => {}) {}"
];

rejected.forEach(function(src) {
  assert.throws(SyntaxError, function() {
    eval(src);
  }, src);
});
