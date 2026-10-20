// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-runtime-semantics-catchclauseevaluation
description: >
  A `using` declaration directly in the Block of a `catch` clause or of a
  `finally` is disposed when that Block completes, with the Block's own
  completion.
info: |
  Catch : catch ( CatchParameter ) Block

  [...]
  7. Let B be Completion(Evaluation of Block).
  8. Set the running execution context's LexicalEnvironment to oldEnv.
  9. Return ? B.

  Block : { StatementList }

  [...]
  4. Let result be Completion(Evaluation of StatementList).
  5. Set result to Completion(DisposeResources(blockEnv.[[DisposeCapability]], result)).

  TryStatement : try Block Finally

  [...]
  2. Let F be Completion(Evaluation of Finally).

  Finally : finally Block

  1. Return ? Evaluation of Block.
features: [explicit-resource-management]
includes: [compareArray.js]
---*/

var log = [];
var resource = { [Symbol.dispose]() { log.push("dispose"); } };
var throwing = { [Symbol.dispose]() { log.push("dispose"); throw "disposer"; } };

function check(fn, expected, label) {
  log = [];
  fn();
  assert.compareArray(log, expected, label);
}

check(function () {
  try { } finally { using r = resource; log.push("finally"); }
  log.push("after");
}, ["finally", "dispose", "after"], "finally, normal try");

check(function () {
  try { throw 0; } catch { using r = resource; log.push("catch"); }
  log.push("after");
}, ["catch", "dispose", "after"], "catch without a parameter");

check(function () {
  try { throw 7; } catch (e) { using r = resource; log.push("catch " + e); }
  log.push("after");
}, ["catch 7", "dispose", "after"], "catch with a parameter");

check(function () {
  try { throw 7; } catch ({ x = 1 }) { using r = resource; log.push("catch " + x); }
  log.push("after");
}, ["catch 1", "dispose", "after"], "catch with a pattern");

check(function () {
  try { throw 0; } catch { using r = resource; log.push("catch"); } finally { log.push("finally"); }
}, ["catch", "dispose", "finally"], "catch disposes before finally");

check(function () {
  try { throw 0; } catch { using r = resource; log.push("catch"); } finally { using s = resource; log.push("finally"); }
}, ["catch", "dispose", "finally", "dispose"], "catch and finally");

check(function () {
  try { using a = { [Symbol.dispose]() { log.push("a"); } }; log.push("try"); }
  finally { using b = { [Symbol.dispose]() { log.push("b"); } }; log.push("finally"); }
}, ["try", "a", "finally", "b"], "try and finally blocks dispose their own resources");

check(function () {
  try { throw 0; } catch {
    using a = { [Symbol.dispose]() { log.push("a"); } };
    using b = { [Symbol.dispose]() { log.push("b"); } };
  }
}, ["b", "a"], "reverse order of declaration");

check(function () {
  try { try { throw 0; } catch { using r = resource; throw 1; } } catch (e) { log.push("outer " + e); }
}, ["dispose", "outer 1"], "catch Block throws");

check(function () {
  try { } finally { using r = resource; try { throw 0; } catch { } }
}, ["dispose"], "finally with a nested try");

var result = (function () {
  log = [];
  try { throw 0; } catch { using r = resource; return "from catch"; }
})();
assert.sameValue(result, "from catch", "return from catch Block");
assert.compareArray(log, ["dispose"], "return from catch Block disposes");

result = (function () {
  log = [];
  try { return "from try"; } finally { using r = resource; log.push("finally"); }
})();
assert.sameValue(result, "from try", "pending return survives finally disposal");
assert.compareArray(log, ["finally", "dispose"], "pending return, finally disposes");

result = (function () {
  log = [];
  try { return "from try"; } finally { using r = resource; return "from finally"; }
})();
assert.sameValue(result, "from finally", "return from finally Block");
assert.compareArray(log, ["dispose"], "return from finally Block disposes");

function leaf() { log.push("call"); return 1; }
result = (function () {
  log = [];
  try { throw 0; } catch { using r = resource; return leaf(); }
})();
assert.sameValue(result, 1, "returned call in catch Block");
assert.compareArray(log, ["call", "dispose"], "returned call runs before the catch disposal");

result = (function () {
  log = [];
  try { } finally { using r = resource; return leaf(); }
})();
assert.sameValue(result, 1, "returned call in finally Block");
assert.compareArray(log, ["call", "dispose"], "returned call runs before the finally disposal");

var loops = 0;
check(function () {
  for (var i = 0; i < 3; i++) {
    try { throw i; } catch { using r = resource; if (i < 2) continue; break; }
  }
}, ["dispose", "dispose", "dispose"], "break and continue out of a catch Block");

check(function () {
  for (var i = 0; i < 2; i++) {
    try { } finally { using r = resource; if (i === 0) continue; break; }
  }
}, ["dispose", "dispose"], "break and continue out of a finally Block");

var caught;
try {
  try { throw "body"; } catch { using r = throwing; log.push("catch"); }
} catch (e) {
  caught = e;
}
assert.sameValue(caught, "disposer", "throwing disposer in a catch Block propagates");

caught = undefined;
try {
  try { throw "body"; } catch { using r = throwing; throw "catch"; }
} catch (e) {
  caught = e;
}
assert.sameValue(caught.constructor, SuppressedError, "disposer after a catch Block throw");
assert.sameValue(caught.error, "disposer", "SuppressedError error");
assert.sameValue(caught.suppressed, "catch", "SuppressedError suppressed");

caught = undefined;
try {
  try { } finally { using r = throwing; log.push("finally"); }
} catch (e) {
  caught = e;
}
assert.sameValue(caught, "disposer", "throwing disposer in a finally Block propagates");

caught = undefined;
try {
  try { throw "try"; } finally { using r = throwing; }
} catch (e) {
  caught = e;
}
assert.sameValue(caught, "disposer", "a finally Block's abrupt completion replaces the pending throw");

check(function () {
  eval("try { throw 0; } catch { using r = resource; log.push('catch'); }");
}, ["catch", "dispose"], "catch Block in eval code");

check(function () {
  eval("try { } finally { using r = resource; log.push('finally'); }");
}, ["finally", "dispose"], "finally Block in eval code");

check(function () {
  (() => { try { throw 0; } catch { using r = resource; log.push("catch"); } })();
}, ["catch", "dispose"], "catch Block in an arrow function");

check(function () {
  var o = { m() { try { throw 0; } catch { using r = resource; log.push("catch"); } } };
  o.m();
}, ["catch", "dispose"], "catch Block in a method");

log = [];
try { throw 0; } catch { using r = resource; log.push("catch"); }
try { } finally { using r = resource; log.push("finally"); }
assert.compareArray(log, ["catch", "dispose", "finally", "dispose"], "script-level try statements");
