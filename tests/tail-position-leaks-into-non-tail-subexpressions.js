// jsse#357: astral-plane identifiers were rejected by esprima's own parser
// running inside jsse, even though jsse's own lexer/parser handled them fine.
// The real bug had nothing to do with Unicode: `Return`'s TCO heuristic
// (`expr_may_contain_tail_call`) over-approximates through a `Conditional`'s
// two branches with `||`, so a ternary whose consequent is a bare call (e.g.
// `String.fromCharCode(cp)`) marks the WHOLE return expression tail-call
// eligible even while evaluating the ALTERNATE branch. Many expression kinds
// then evaluated their own non-tail sub-expressions (operands, elements,
// property values, computed keys, `new`/`import()` arguments, template
// substitutions, assignment/update targets, class computed keys, optional
// chains) without clearing `in_tail_position` first, so a call nested in the
// branch that actually executes got mistaken for the tail call: it was
// returned as a `Completion::TailCall` and everything after it in that
// expression was silently skipped. Esprima's `Character.fromCodePoint` hit
// this exactly: `(cp < 0x10000) ? String.fromCharCode(cp) :
// String.fromCharCode(A) + String.fromCharCode(B)` — the alternate's SECOND
// `fromCharCode` call never ran, truncating every astral surrogate pair to
// its high half.
//
// Fixed at the root: `eval_expr` now captures the ambient tail-call
// eligibility once at entry and clears it by default, so every arm evaluates
// as non-tail unless it is one of the handful of genuine tail positions
// (Conditional's taken branch, Logical's short-circuited right operand,
// Sequence's last element, Call, TaggedTemplate) that explicitly restore it
// right before their own recursive dispatch. This covers every expression
// kind by construction instead of requiring each one to remember to clear it.
//
// Only reproduces in strict mode (TCO is gated on `env.borrow().strict`).

"use strict";

function fromCodePoint(cp) {
  return cp < 0x10000
    ? String.fromCharCode(cp)
    : String.fromCharCode(0xd800 + ((cp - 0x10000) >> 10)) +
        String.fromCharCode(0xdc00 + ((cp - 0x10000) & 1023));
}

var s = fromCodePoint(0x1e800);
if (s.length !== 2 || s.charCodeAt(0) !== 0xd83a || s.charCodeAt(1) !== 0xdc00) {
  throw new Error(
    "Binary(+) operand wrongly treated as tail call: length=" +
      s.length +
      " codes=[" +
      Array.from(s)
        .map((_, i) => s.charCodeAt(i).toString(16))
        .join(",") +
      "]"
  );
}

// Array literal element.
(function () {
  function f(c) {
    return c ? String.fromCharCode(65) : [String.fromCharCode(66)];
  }
  var r = f(false);
  if (!Array.isArray(r) || r.length !== 1 || r[0] !== "B") {
    throw new Error("Array literal element wrongly treated as tail call: " + JSON.stringify(r));
  }
})();

// Object literal property value.
(function () {
  function f(c) {
    return c ? String.fromCharCode(65) : { k: String.fromCharCode(66) };
  }
  var r = f(false);
  if (typeof r !== "object" || r.k !== "B") {
    throw new Error("Object literal property wrongly treated as tail call: " + JSON.stringify(r));
  }
})();

// Computed member property key.
(function () {
  var o = { B: 42 };
  function f(c) {
    return c ? String.fromCharCode(65) : o[String.fromCharCode(66)];
  }
  var r = f(false);
  if (r !== 42) {
    throw new Error("Computed member key wrongly treated as tail call: " + r);
  }
})();

// `new` constructor argument.
(function () {
  function K(x) {
    this.v = x;
  }
  function f(c) {
    return c ? String.fromCharCode(65) : new K(String.fromCharCode(66));
  }
  var r = f(false);
  if (r.v !== "B") {
    throw new Error("`new` argument wrongly treated as tail call: " + JSON.stringify(r));
  }
})();

// Template literal substitution.
(function () {
  function f(c) {
    return c ? String.fromCharCode(65) : `x${String.fromCharCode(66)}y`;
  }
  var r = f(false);
  if (r !== "xBy") {
    throw new Error("Template substitution wrongly treated as tail call: " + JSON.stringify(r));
  }
})();

// Optional-chain computed property key.
(function () {
  var o = { B: 42 };
  function f(c) {
    return c ? String.fromCharCode(65) : o?.[String.fromCharCode(66)];
  }
  var r = f(false);
  if (r !== 42) {
    throw new Error("OptionalChain computed key wrongly treated as tail call: " + r);
  }
})();

// Optional-chain call argument.
(function () {
  function g(x) {
    return x + 1;
  }
  function f(c) {
    return c ? String.fromCharCode(65) : g?.(String.fromCharCode(66).charCodeAt(0));
  }
  var r = f(false);
  if (r !== 67) {
    throw new Error("OptionalChain call argument wrongly treated as tail call: " + r);
  }
})();

// Computed assignment target.
(function () {
  var o = {};
  function f(c) {
    return c ? String.fromCharCode(65) : (o[String.fromCharCode(66)] = 99);
  }
  var r = f(false);
  if (r !== 99 || o.B !== 99) {
    throw new Error(
      "Computed assignment target wrongly treated as tail call: " + JSON.stringify(o) + " r=" + r
    );
  }
})();

// Update (++) on a computed member.
(function () {
  var o = { B: 5 };
  function f(c) {
    return c ? String.fromCharCode(65) : o[String.fromCharCode(66)]++;
  }
  var r = f(false);
  if (r !== 5 || o.B !== 6) {
    throw new Error(
      "Update target wrongly treated as tail call: " + JSON.stringify(o) + " r=" + r
    );
  }
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
  if (typeof r.B !== "function" || r.B() !== "ok") {
    throw new Error("Class computed method key wrongly treated as tail call");
  }
})();

// Dynamic import() specifier.
(function () {
  function f(c) {
    return c ? String.fromCharCode(65) : import(String.fromCharCode(66) + "://nonexistent");
  }
  var r = f(false);
  if (typeof r.then !== "function") {
    throw new Error("import() specifier wrongly treated as tail call");
  }
})();

// A genuine tail call must still be optimized (no stack growth), including
// through the other real tail positions (Sequence's last element, Logical's
// right operand).
(function () {
  function count(n, acc) {
    "use strict";
    if (n <= 0) return acc;
    return count(n - 1, acc + 1);
  }
  if (count(200000, 0) !== 200000) {
    throw new Error("proper tail call regressed");
  }
})();

(function () {
  function count(n, acc) {
    "use strict";
    return n <= 0 ? acc : (0, count(n - 1, acc + 1));
  }
  if (count(200000, 0) !== 200000) {
    throw new Error("proper tail call through comma operator regressed");
  }
})();

(function () {
  function count(n, acc) {
    "use strict";
    if (n <= 0) return acc;
    return false || count(n - 1, acc + 1);
  }
  if (count(200000, 0) !== 200000) {
    throw new Error("proper tail call through logical operator regressed");
  }
})();
