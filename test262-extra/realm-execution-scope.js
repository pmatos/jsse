/*---
description: Realm selection and caller restoration across calls and resumptions
esid: sec-prepareforordinarycall
info: |
  PrepareForOrdinaryCall installs the function's Realm. Returning or suspending
  restores the caller's execution context. GeneratorResume and Await restore
  the suspended execution context when execution continues.
flags: [async]
features: [cross-realm, generators, async-functions, async-iteration]
---*/

var foreign = $262.createRealm().global;
var foreignArrayPrototype = foreign.Array.prototype;

function localArray(value, message) {
  assert.sameValue(Object.getPrototypeOf(value), Array.prototype, message);
}

function foreignArray(value, message) {
  assert.sameValue(Object.getPrototypeOf(value), foreignArrayPrototype, message);
}

function callerRestored(value) {
  localArray(value, "caller array after foreign execution");
  assert.throws(TypeError, function () { null.x; });
}

// Repeated calls include the compiled simple, arguments, and default paths.
var functions = [
  foreign.eval("(function () { return []; })"),
  foreign.eval("(function (x) { arguments[0]; return []; })"),
  foreign.eval("(function (x = 1) { return []; })")
];
functions.forEach(function (fn) {
  for (var i = 0; i < 4; i++) {
    foreignArray(fn(), "ordinary call owns its realm");
    callerRestored([]);
  }
});

var nested = foreign.eval(
  "(function (callback) { var v = callback(); $262.gc(); return [v, []]; })"
);
var nestedResult = nested(function () {
  callerRestored([]);
  return [];
});
foreignArray(nestedResult, "foreign call resumes after nested local call");
localArray(nestedResult[0], "nested callback executes in its own realm");
foreignArray(nestedResult[1], "foreign body retains its realm");
callerRestored([]);

var failures = [
  foreign.eval("(function () { null.x; })"),
  foreign.eval("(function (x = null.x) { return []; })"),
  foreign.eval("(class ForeignClass {})")
];
for (var warm = 0; warm < 4; warm++) {
  foreignArray(failures[1](1), "warm default-parameter call");
}
failures.forEach(function (fn) {
  for (var i = 0; i < 3; i++) {
    var caught = false;
    try {
      fn();
    } catch (error) {
      caught = true;
      assert.sameValue(error instanceof foreign.TypeError, true);
      callerRestored([]);
    }
    assert.sameValue(caught, true, "foreign call must throw");
  }
});
assert.throws(foreign.TypeError, function () {
  try {
    foreign.Array.prototype.map.call(null, function () {});
  } finally {
    callerRestored([]);
  }
});
callerRestored([]);
foreignArray(foreign.Function("return [];")(), "dynamic Function realm");
foreignArray(foreign.eval("[]"), "indirect eval realm");
callerRestored([]);

var makeGenerator = foreign.eval(
  "(function* () { try { yield []; } catch (e) { yield []; } finally { yield []; } })"
);
var generator = makeGenerator();
foreignArray(generator.next().value, "generator next realm");
callerRestored([]);
foreignArray(generator.throw(1).value, "generator throw realm");
callerRestored([]);
foreignArray(generator.return(2).value, "generator return finalizer realm");
callerRestored([]);
assert.sameValue(generator.next().value, 2);
callerRestored([]);

var asyncFunction = foreign.eval(
  "(async function (callback) { await 1; callback(); $262.gc(); return []; })"
);
var makeAsyncGenerator = foreign.eval(
  "(async function* () { try { await 1; yield []; } finally { await 1; yield []; } })"
);
var asyncGenerator = makeAsyncGenerator();
var asyncChecks = asyncFunction(function () { callerRestored([]); }).then(function (value) {
  foreignArray(value, "async function continuation realm");
  callerRestored([]);
});
var generatorChecks = asyncGenerator.next().then(function (result) {
  foreignArray(result.value, "async generator continuation realm");
  callerRestored([]);
  return asyncGenerator.return(3);
}).then(function (result) {
  foreignArray(result.value, "async generator finalizer realm");
  callerRestored([]);
  return asyncGenerator.next();
}).then(function (result) {
  assert.sameValue(result.value, 3);
  assert.sameValue(result.done, true);
  callerRestored([]);
});
var rejectionChecks = foreign.eval("(async function () { await 1; null.x; })")().then(
  function () { throw new Test262Error("expected rejection"); },
  function (error) {
    assert.sameValue(error instanceof foreign.TypeError, true);
    callerRestored([]);
  }
);
var throwingGenerator = foreign.eval(
  "(async function* () { try { yield []; } catch (e) { await 1; null.x; } })"
)();
var throwChecks = throwingGenerator.next().then(function (result) {
  foreignArray(result.value, "async generator before throw");
  callerRestored([]);
  return throwingGenerator.throw(1);
}).then(
  function () { throw new Test262Error("expected generator rejection"); },
  function (error) {
    assert.sameValue(error instanceof foreign.TypeError, true);
    callerRestored([]);
  }
);
Promise.all([asyncChecks, generatorChecks, rejectionChecks, throwChecks]).then(function () {}).then($DONE, $DONE);
