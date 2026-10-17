// Copyright (C) 2026 the JSSE authors. All rights reserved.
/*---
esid: sec-ecmascript-function-objects-call-thisargument-argumentslist
description: >
  A function that has been called a few times is served by the engine's
  compiled-call path. Every observable part of [[Call]] must be the same on the
  tenth call as on the first: `this` coercion, Function.prototype.arguments and
  caller, class constructors and Proxy objects that share a callee's code,
  closures that capture parameters, hoisted declarations in the body, tail
  calls, exceptions, realms and stack exhaustion.
features: [Proxy, class, cross-realm, host-gc-required]
flags: [noStrict]
---*/

function times(n, f) {
  for (var i = 0; i < n; i++) f(i);
}

function sloppyThis() {
  return this;
}
function strictThis() {
  "use strict";
  return this;
}
times(6, function (i) {
  assert.sameValue(sloppyThis(), globalThis, "sloppy this, call " + i);
  assert.sameValue(typeof sloppyThis.call(i), "object", "sloppy primitive this, call " + i);
  assert.sameValue(strictThis.call(i), i, "strict primitive this, call " + i);
  assert.sameValue(strictThis(), undefined, "strict undefined this, call " + i);
});

function reader(a) {
  return reader["argu" + "ments"];
}
times(6, function (i) {
  var record = reader(i, "extra");
  assert.sameValue(record.length, 2, "arguments length, call " + i);
  assert.sameValue(record[0], i, "arguments[0], call " + i);
  assert.sameValue(record[1], "extra", "arguments[1], call " + i);
});
assert.sameValue(reader["argu" + "ments"], null, "no record outside an activation");

function callerProbe() {
  return callerProbe["cal" + "ler"];
}
function outerCaller() {
  return callerProbe();
}
times(6, function (i) {
  assert.sameValue(outerCaller(), outerCaller, "Function.prototype.caller, call " + i);
});

class Point {
  constructor(x) {
    this.x = x;
  }
}
times(6, function (i) {
  assert.sameValue(new Point(i).x, i, "construct, call " + i);
  assert.throws(TypeError, function () {
    Point(i);
  }, "class constructor call without new, call " + i);
});

function plain(x) {
  return x + 1;
}
var trapCalls = 0;
var proxied = new Proxy(plain, {
  apply: function (target, thisArg, args) {
    trapCalls++;
    return target.apply(thisArg, args) * 10;
  }
});
times(6, function (i) {
  assert.sameValue(plain(i), i + 1, "plain call " + i);
  assert.sameValue(proxied(i), (i + 1) * 10, "proxy call goes through the trap, call " + i);
});
assert.sameValue(trapCalls, 6, "the apply trap ran once per proxy call");

var revocable = Proxy.revocable(plain, {});
times(3, function () {
  plain(1);
});
revocable.revoke();
assert.throws(TypeError, function () {
  revocable.proxy(1);
}, "revoked proxy of a warmed-up function");

function capture(a, b) {
  return function () {
    return a + b;
  };
}
var captured = [];
times(6, function (i) {
  captured.push(capture(i, 100));
});
captured.forEach(function (f, i) {
  assert.sameValue(f(), i + 100, "captured parameters, closure " + i);
});

function hoisted(n) {
  var first = helper(n);
  function helper(x) {
    return x * 2;
  }
  return first + (typeof later);
  var later;
}
times(6, function (i) {
  assert.sameValue(hoisted(i), i * 2 + "undefined", "hoisted declarations, call " + i);
});

function paramMutation(a, b) {
  a = a + 1;
  b = (b === undefined) ? "default" : b;
  return a + ":" + b;
}
times(6, function (i) {
  assert.sameValue(paramMutation(i), i + 1 + ":default", "missing argument, call " + i);
  assert.sameValue(paramMutation(i, "b", "extra"), i + 1 + ":b", "extra argument, call " + i);
});

function strictTail(n, acc) {
  "use strict";
  return n === 0 ? acc : strictTail(n - 1, acc + 1);
}
times(4, function (i) {
  assert.sameValue(strictTail(200, i), 200 + i, "strict tail calls, call " + i);
});

function boom(i) {
  try {
    throw new Error("boom " + i);
  } finally {
    boom.finalized = i;
  }
}
times(6, function (i) {
  var message;
  try {
    boom(i);
  } catch (e) {
    message = e.message;
  }
  assert.sameValue(message, "boom " + i, "exception from a warmed-up function, call " + i);
  assert.sameValue(boom.finalized, i, "finally ran, call " + i);
});

function recurse(n) {
  return recurse(n + 1) + 1;
}
times(4, function (i) {
  assert.throws(RangeError, function () {
    recurse(0);
  }, "stack exhaustion is catchable, round " + i);
  assert.sameValue(plain(i), i + 1, "calls work after the overflow, round " + i);
});

var other = $262.createRealm().global;
other.eval("function whoAmI() { return this; } function make() { return new Error('x'); }");
var otherThis = other.whoAmI;
var otherMake = other.make;
times(6, function (i) {
  assert.sameValue(otherThis(), other, "sloppy this is the callee's realm global, call " + i);
  assert.sameValue(
    Object.getPrototypeOf(otherMake()),
    other.Error.prototype,
    "errors are created in the callee's realm, call " + i
  );
});

function counter() {
  counter.n = (counter.n || 0) + 1;
  return counter.n;
}
times(6, function (i) {
  assert.sameValue(counter(), i + 1, "function object properties, call " + i);
});

function gcProbe(a, b) {
  $262.gc();
  return a.tag + b.tag;
}
times(6, function (i) {
  assert.sameValue(gcProbe({ tag: "x" }, { tag: i }), "x" + i, "arguments survive a collection, call " + i);
});
