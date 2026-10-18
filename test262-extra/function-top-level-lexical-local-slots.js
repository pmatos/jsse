/*---
description: Function-top-level let/const/class bindings read and written through frame slots keep TDZ, const and closure semantics
info: |
  10.2.11 FunctionDeclarationInstantiation steps 27-36
  9.1.1.1.1 HasBinding / 9.1.1.1.6 GetBindingValue / 9.1.1.1.5 SetMutableBinding

  Whatever order the function's own bindings are created in (parameters,
  `arguments`, var names, function declarations, lexical declarations), a
  lexical binding stays uninitialised until its declaration is evaluated,
  `const` stays immutable, and closures observe every write.
esid: sec-functiondeclarationinstantiation
flags: [noStrict]
---*/

function tdzRead() {
  var before;
  try {
    before = x;
  } catch (e) {
    before = e.constructor;
  }
  let x = 1;
  return before === ReferenceError && x === 1;
}
assert.sameValue(tdzRead(), true, "read before the declaration throws, after it succeeds");

function tdzTypeof() {
  var seen;
  try {
    seen = typeof x;
  } catch (e) {
    seen = e.constructor;
  }
  const x = 1;
  return seen === ReferenceError && typeof x === "number";
}
assert.sameValue(tdzTypeof(), true, "typeof of an uninitialised lexical throws");

function tdzWrite() {
  var threw = false;
  try {
    x = 1;
  } catch (e) {
    threw = e instanceof ReferenceError;
  }
  let x = 0;
  return threw && x === 0;
}
assert.sameValue(tdzWrite(), true, "assignment before the declaration throws and leaves it uninitialised");

function constWriteOrder() {
  var log = [];
  const c = 1;
  try {
    c = (log.push("rhs"), 2);
  } catch (e) {
    log.push(e.constructor === TypeError ? "TypeError" : "other");
  }
  try {
    c += (log.push("rhs2"), 2);
  } catch (e) {
    log.push(e.constructor === TypeError ? "TypeError" : "other");
  }
  try {
    c++;
  } catch (e) {
    log.push(e.constructor === TypeError ? "TypeError" : "other");
  }
  return log.join() + ":" + c;
}
assert.sameValue(
  constWriteOrder(),
  "rhs,TypeError,rhs2,TypeError,TypeError:1",
  "the right-hand side is evaluated before the const write throws"
);

function declarationsBetween(a, b) {
  var v = a;
  function first() { return l1 + c1; }
  let l1 = 10;
  function second() { return l1 * 2; }
  const c1 = b;
  class K { static get n() { return l1 + c1; } }
  var w = l1 + c1 + K.n;
  l1 += 5;
  return [first(), second(), w, l1, c1, v].join();
}
assert.sameValue(declarationsBetween(1, 2), "17,30,24,15,2,1", "mixed declarations resolve to their own bindings");

function closureSees() {
  let counter = 0;
  const bump = function () { counter++; return counter; };
  bump();
  counter += 10;
  bump();
  return counter;
}
assert.sameValue(closureSees(), 12, "closure and body share one binding");

function shadowing() {
  let x = 1;
  {
    let x = 2;
    x += 1;
  }
  for (let x = 10; x < 11; x++) {}
  return x;
}
assert.sameValue(shadowing(), 1, "block and loop lets shadow the function-level one");

function labelledFunctionAndLet() {
  lbl: function inner() { return z; }
  let z = 5;
  return inner();
}
assert.sameValue(labelledFunctionAndLet(), 5, "labelled function declaration before a let");

function withDestructuring() {
  let { p, q } = { p: 1, q: 2 };
  let r = p + q;
  r *= 2;
  return p + "," + q + "," + r;
}
assert.sameValue(withDestructuring(), "1,2,6", "pattern lexicals sit beside slot-addressed ones");

function varNotInPrologue() {
  if (false) { var hidden = 1; }
  let after = 3;
  after++;
  return after + ":" + typeof hidden;
}
assert.sameValue(varNotInPrologue(), "4:undefined", "var names declared elsewhere do not disturb lexical access");

function* generatorLexicals(n) {
  let total = 0;
  const step = 2;
  for (var i = 0; i < n; i++) {
    total += step;
    yield total;
  }
  return total;
}
var g = generatorLexicals(3);
assert.sameValue(g.next().value + g.next().value + g.next().value, 12, "generator body reads its lexicals across yields");

function reentrant(n) {
  let local = n;
  if (n > 0) reentrant(n - 1);
  local += 1;
  return local;
}
assert.sameValue(reentrant(3), 4, "each activation has its own lexical bindings");

function repeated() {
  let acc = 0;
  for (var i = 0; i < 100; i++) {
    acc += i & 3;
  }
  return acc;
}
assert.sameValue(repeated(), 150, "slot-addressed lexical accumulates across a loop");
assert.sameValue(repeated(), 150, "and again on the next activation");
