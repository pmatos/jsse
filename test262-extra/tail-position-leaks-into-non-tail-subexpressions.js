/*---
description: Calls nested in non-tail subexpressions do not become tail calls
esid: sec-static-semantics-hascallintailposition
info: |
  HasCallInTailPosition identifies only the descendants named by its grammar
  productions. Calls in binary operands, array or object literals, computed
  keys, new or call arguments, template substitutions, assignment or update
  targets, class computed keys, and import() specifiers are not tail calls.

  This is regression coverage for an implementation that over-approximated a
  conditional branch as containing a tail call and then leaked that eligibility
  into non-tail subexpressions in the branch that actually executed. Genuine
  tail positions in conditional, sequence, and logical expressions must remain
  eligible for proper tail calls.
flags: [onlyStrict]
features: [tail-call-optimization, dynamic-import]
---*/

"use strict";

function fromCodePoint(cp) {
  return cp < 0x10000
    ? String.fromCharCode(cp)
    : String.fromCharCode(0xd800 + ((cp - 0x10000) >> 10)) +
        String.fromCharCode(0xdc00 + ((cp - 0x10000) & 1023));
}

var s = fromCodePoint(0x1e800);
assert.sameValue(s.length, 2, "both binary-expression operands are evaluated");
assert.sameValue(s.charCodeAt(0), 0xd83a, "the high surrogate is preserved");
assert.sameValue(s.charCodeAt(1), 0xdc00, "the low surrogate is preserved");

// Array literal element.
(function () {
  function f(c) {
    return c ? String.fromCharCode(65) : [String.fromCharCode(66)];
  }
  var r = f(false);
  assert.sameValue(Array.isArray(r), true, "an array literal remains an array");
  assert.sameValue(r.length, 1, "the array literal contains its element");
  assert.sameValue(r[0], "B", "the array literal element call is evaluated normally");
})();

// Object literal property value.
(function () {
  function f(c) {
    return c ? String.fromCharCode(65) : { k: String.fromCharCode(66) };
  }
  var r = f(false);
  assert.sameValue(typeof r, "object", "an object literal remains an object");
  assert.sameValue(r.k, "B", "the object property value call is evaluated normally");
})();

// Computed member property key.
(function () {
  var o = { B: 42 };
  function f(c) {
    return c ? String.fromCharCode(65) : o[String.fromCharCode(66)];
  }
  assert.sameValue(f(false), 42, "the computed member key call is evaluated normally");
})();

// `new` constructor argument.
(function () {
  function K(x) {
    this.v = x;
  }
  function f(c) {
    return c ? String.fromCharCode(65) : new K(String.fromCharCode(66));
  }
  assert.sameValue(f(false).v, "B", "the constructor argument call is evaluated normally");
})();

// Template literal substitution.
(function () {
  function f(c) {
    return c ? String.fromCharCode(65) : `x${String.fromCharCode(66)}y`;
  }
  assert.sameValue(f(false), "xBy", "the template substitution call is evaluated normally");
})();

// Optional-chain computed property key.
(function () {
  var o = { B: 42 };
  function f(c) {
    return c ? String.fromCharCode(65) : o?.[String.fromCharCode(66)];
  }
  assert.sameValue(f(false), 42, "the optional-chain computed key call is evaluated normally");
})();

// Optional-chain call argument.
(function () {
  function g(x) {
    return x + 1;
  }
  function f(c) {
    return c ? String.fromCharCode(65) : g?.(String.fromCharCode(66).charCodeAt(0));
  }
  assert.sameValue(f(false), 67, "the optional-chain argument call is evaluated normally");
})();

// Computed assignment target.
(function () {
  var o = {};
  function f(c) {
    return c ? String.fromCharCode(65) : (o[String.fromCharCode(66)] = 99);
  }
  var r = f(false);
  assert.sameValue(r, 99, "the assignment expression produces the assigned value");
  assert.sameValue(o.B, 99, "the computed assignment target call is evaluated normally");
})();

// Update (++) on a computed member.
(function () {
  var o = { B: 5 };
  function f(c) {
    return c ? String.fromCharCode(65) : o[String.fromCharCode(66)]++;
  }
  var r = f(false);
  assert.sameValue(r, 5, "the postfix update produces the previous value");
  assert.sameValue(o.B, 6, "the computed update target call is evaluated normally");
})();

// Class expression computed method key.
(function () {
  function f(c) {
    return c
      ? String.fromCharCode(65)
      : new (class {
          [String.fromCharCode(66)]() {
            return "ok";
          }
        })();
  }
  var r = f(false);
  assert.sameValue(typeof r.B, "function", "the computed class method is installed");
  assert.sameValue(r.B(), "ok", "the computed class method key call is evaluated normally");
})();

// Dynamic import() specifier.
(function () {
  function f(c) {
    return c ? String.fromCharCode(65) : import(String.fromCharCode(66) + "://nonexistent");
  }
  var r = f(false);
  assert.sameValue(typeof r.then, "function", "the import specifier call is evaluated normally");
  r.catch(function () {});
})();

// Genuine proper tail call.
(function () {
  function count(n, acc) {
    "use strict";
    if (n <= 0) return acc;
    return count(n - 1, acc + 1);
  }
  assert.sameValue(count(200000, 0), 200000, "a direct proper tail call remains optimized");
})();

// Genuine proper tail call through a sequence expression.
(function () {
  function count(n, acc) {
    "use strict";
    return n <= 0 ? acc : (0, count(n - 1, acc + 1));
  }
  assert.sameValue(count(200000, 0), 200000, "a sequence tail call remains optimized");
})();

// Genuine proper tail call through a logical expression.
(function () {
  function count(n, acc) {
    "use strict";
    if (n <= 0) return acc;
    return false || count(n - 1, acc + 1);
  }
  assert.sameValue(count(200000, 0), 200000, "a logical-expression tail call remains optimized");
})();
