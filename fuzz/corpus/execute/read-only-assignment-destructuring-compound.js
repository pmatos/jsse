// Characterization of the [[Set]] path taken by destructuring-assignment and
// compound-assignment targets (Interpreter::set_object_with_key). It pins both
// the OrdinarySet *semantics* (spec §10.1.9 / §6.2.5.6 PutValue — the source of
// truth for the throw/no-throw and value assertions) and jsse's host-compatible
// strict-reject *diagnostics* (the exact TypeError messages, pinned as a
// host-compat regression the way tests/read-only-assignment-receiver-diagnostic.js
// pins the plain-assignment path).
//
// This path is distinct from the plain `obj.x = v` path: the truncated
// "Cannot assign to read only property 'x'" message (no "of object '#<Object>'"
// suffix) below is the fingerprint that identifies this path.

function fail(msg) {
  throw new Error("FAIL: " + msg);
}
function assert(cond, msg) {
  if (!cond) fail(msg);
}
function assertThrows(fn, name, message, label) {
  var threw = false;
  try {
    fn();
  } catch (e) {
    threw = true;
    if (e.constructor.name !== name) {
      fail(label + ": expected " + name + " but got " + e.constructor.name + " (" + e.message + ")");
    }
    if (message !== undefined && e.message !== message) {
      fail(label + ": expected message " + JSON.stringify(message) + " but got " + JSON.stringify(e.message));
    }
  }
  if (!threw) fail(label + ": expected a throw but none occurred");
}

// ---- Strict mode: read-only / getter-only targets reject with TypeError ----
(function () {
  "use strict";

  // Destructuring -> frozen own data property.
  (function () {
    var o = Object.freeze({ x: 1 });
    assertThrows(
      function () { [o.x] = [5]; },
      "TypeError",
      "Cannot assign to read only property 'x'",
      "destructuring frozen own data"
    );
    assert(o.x === 1, "frozen own data left unchanged");
  })();

  // Destructuring -> inherited non-writable data property.
  (function () {
    var proto = Object.defineProperty({}, "x", { value: 1, writable: false });
    var o = Object.create(proto);
    assertThrows(
      function () { [o.x] = [5]; },
      "TypeError",
      "Cannot assign to read only property 'x'",
      "destructuring inherited non-writable data"
    );
    assert(!Object.prototype.hasOwnProperty.call(o, "x"), "no own x created on receiver");
  })();

  // Destructuring -> own getter-only accessor.
  (function () {
    var o = { get x() { return 1; } };
    assertThrows(
      function () { [o.x] = [5]; },
      "TypeError",
      "Cannot set property 'x' which has only a getter",
      "destructuring own getter-only accessor"
    );
  })();

  // Destructuring -> inherited getter-only accessor.
  (function () {
    var proto = { get x() { return 1; } };
    var o = Object.create(proto);
    assertThrows(
      function () { [o.x] = [5]; },
      "TypeError",
      "Cannot set property 'x' which has only a getter",
      "destructuring inherited getter-only accessor"
    );
  })();

  // Compound assignment -> frozen own data property.
  (function () {
    var o = Object.freeze({ x: 1 });
    assertThrows(
      function () { o.x += 1; },
      "TypeError",
      "Cannot assign to read only property 'x'",
      "compound frozen own data"
    );
    assert(o.x === 1, "compound frozen own data left unchanged");
  })();

  // Compound assignment -> own getter-only accessor.
  (function () {
    var o = { get x() { return 1; } };
    assertThrows(
      function () { o.x += 1; },
      "TypeError",
      "Cannot set property 'x' which has only a getter",
      "compound own getter-only accessor"
    );
  })();
})();

// ---- Strict mode: successful sets go through the same path ----
(function () {
  "use strict";

  // Destructuring invokes an own setter with the assigned value.
  (function () {
    var got;
    var o = { set x(v) { got = v; } };
    [o.x] = [42];
    assert(got === 42, "destructuring own setter received value");
  })();

  // Destructuring invokes an inherited setter.
  (function () {
    var got;
    var proto = { set x(v) { got = v; } };
    var o = Object.create(proto);
    [o.x] = [7];
    assert(got === 7, "destructuring inherited setter received value");
  })();

  // Destructuring overwrites a writable own data property.
  (function () {
    var o = { x: 1 };
    [o.x] = [5];
    assert(o.x === 5, "destructuring writable own data set");
  })();

  // Destructuring creates a new own property on an extensible object.
  (function () {
    var o = {};
    [o.y] = [9];
    assert(o.y === 9, "destructuring created new own property");
  })();

  // Destructuring writes through a typed-array integer index.
  (function () {
    var ta = new Int8Array(2);
    [ta[0]] = [9];
    assert(ta[0] === 9, "destructuring typed-array index set");
  })();

  // Compound assignment reads via the getter and writes via the setter.
  (function () {
    var seen;
    var o = { get x() { return 10; }, set x(v) { seen = v; } };
    o.x += 5;
    assert(seen === 15, "compound accessor read then write");
  })();
})();

// ---- Sloppy mode: rejected [[Set]] is a silent no-op, not a throw ----
(function () {
  var o = Object.freeze({ x: 1 });
  [o.x] = [5];
  assert(o.x === 1, "sloppy destructuring read-only silently ignored");

  var o2 = Object.freeze({ x: 1 });
  o2.x += 1;
  assert(o2.x === 1, "sloppy compound read-only silently ignored");
})();
