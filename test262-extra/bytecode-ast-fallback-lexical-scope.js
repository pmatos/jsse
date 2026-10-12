/*---
description: >
  let/const/class declarations inside a compiled function keep their block
  scope, temporal dead zone and per-iteration bindings even though the
  statements that own them run on the tree-walker.
info: |
  jsse issue #902: the VM has no block environments, so a block, loop or
  top-level statement that declares a lexical binding is handed back to the
  tree-walker as a unit. The observable scoping must be unchanged.

  BlockDeclarationInstantiation (14.2.3), let and const declarations
  runtime semantics (14.3.1.2), CreatePerIterationEnvironment (14.7.4.4).
esid: sec-blockdeclarationinstantiation
flags: [noStrict]
---*/

function blockScopedLetDoesNotLeak() {
  var x = "outer";
  {
    let x = "inner";
    if (x !== "inner") return "bad-inner";
  }
  return x;
}
assert.sameValue(blockScopedLetDoesNotLeak(), "outer");

function loopClosuresCapturePerIterationBinding() {
  var fns = [];
  for (let i = 0; i < 3; i++) {
    fns.push(function () { return i; });
  }
  return fns.map(function (f) { return f(); }).join(",");
}
assert.sameValue(loopClosuresCapturePerIterationBinding(), "0,1,2");

function compiledLoopWithLetBlockBody() {
  var sum = 0;
  var fns = [];
  for (var k = 0; k < 3; k++) {
    let captured = k * 10;
    fns.push(function () { return captured; });
    sum += k;
  }
  return sum + ":" + fns.map(function (f) { return f(); }).join(",");
}
assert.sameValue(compiledLoopWithLetBlockBody(), "3:0,10,20");

function topLevelTdzRead() {
  var caught;
  try {
    var before = lateLet;
  } catch (e) {
    caught = e.name;
  }
  let lateLet = 1;
  return caught + ":" + lateLet;
}
assert.sameValue(topLevelTdzRead(), "ReferenceError:1");

function typeofInTdzThrows() {
  var caught;
  try {
    typeof tdz;
  } catch (e) {
    caught = e.name;
  }
  let tdz = 1;
  return caught;
}
assert.sameValue(typeofInTdzThrows(), "ReferenceError");

function constAssignmentThrows() {
  const c = 1;
  var caught;
  try {
    c = 2;
  } catch (e) {
    caught = e.name;
  }
  return caught + ":" + c;
}
assert.sameValue(constAssignmentThrows(), "TypeError:1");

function classDeclarationInFunction() {
  var made = new C(3).value;
  class C {
    constructor(v) { this.value = v; }
  }
  return made;
}
assert.throws(ReferenceError, classDeclarationInFunction);

function classDeclarationAfterUse() {
  class C {
    constructor(v) { this.value = v; }
  }
  return new C(3).value;
}
assert.sameValue(classDeclarationAfterUse(), 3);

function lexicalInIfBlockDoesNotLeak() {
  var seen = typeof leak;
  if (seen === "undefined") {
    let leak = 1;
  }
  return seen + ":" + typeof leak;
}
assert.sameValue(lexicalInIfBlockDoesNotLeak(), "undefined:undefined");

function catchParameterScope() {
  var e = "outer";
  try {
    throw "inner";
  } catch (e) {
    if (e !== "inner") return "bad";
  }
  return e;
}
assert.sameValue(catchParameterScope(), "outer");
