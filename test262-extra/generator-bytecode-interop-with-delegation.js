/*---
description: >
  Generators interoperate regardless of how the engine runs their bodies:
  yield* delegating to, from and between generators (return and throw are
  forwarded to the inner generator), spread, destructuring, Array.from and
  iterator helpers consuming them, and generators whose bodies hold
  constructs a suspending frame cannot host (yield* itself, destructuring
  defaults and templates containing yield).
esid: sec-generator-function-definitions-runtime-semantics-evaluation
features: [generators, Symbol.iterator]
---*/

function drain(it, sends) {
  const out = [];
  let r, k = 0;
  while (true) {
    r = it.next(sends ? sends[k++] : undefined);
    out.push(r.done ? "R:" + String(r.value) : "Y:" + String(r.value));
    if (r.done || out.length > 60) break;
  }
  return out.join(",");
}

function* plain(tag, n) { for (var i = 0; i < n; i++) yield tag + i; return tag + "-done"; }

// yield* of a generator forwards values, sent values and the return value.
function* delegating() {
  var r1 = yield* plain("a", 2);
  var r2 = yield* plain("b", 1);
  return r1 + "/" + r2;
}
assert.sameValue(drain(delegating()), "Y:a0,Y:a1,Y:b0,R:a-done/b-done");

function* echoInner() { var x = yield "ready"; var y = yield "got " + x; return x + y; }
function* echoOuter() { return yield* echoInner(); }
assert.sameValue(drain(echoOuter(), [0, "A", "B"]), "Y:ready,Y:got A,R:AB");

// return() and throw() reach the inner generator's finally and catch blocks.
var log = [];
function* inner() {
  try { yield 1; yield 2; } catch (e) { log.push("inner caught " + e); yield "recovered"; } finally { log.push("inner finally"); }
}
function* outer() { try { yield* inner(); } finally { log.push("outer finally"); } }
var it = outer();
it.next();
assert.sameValue(it.return("r").value, "r");
assert.sameValue(log.join(), "inner finally,outer finally");
log = [];
it = outer();
it.next();
assert.sameValue(it.throw("boom").value, "recovered");
assert.sameValue(it.next().done, true);
assert.sameValue(log.join(), "inner caught boom,inner finally,outer finally");

// A generator consumed by spread, destructuring, Array.from and for-of.
assert.sameValue([...plain("s", 3)].join(), "s0,s1,s2");
var [d0, d1, ...drest] = plain("d", 4);
assert.sameValue([d0, d1, drest.join()].join("|"), "d0|d1|d2,d3");
assert.sameValue(Array.from(plain("f", 2), x => x + "!").join(), "f0!,f1!");
var seen = [];
for (const v of plain("l", 3)) seen.push(v);
assert.sameValue(seen.join(), "l0,l1,l2");

// Destructuring closes a generator it did not exhaust.
log = [];
function* closable() { try { yield 1; yield 2; yield 3; } finally { log.push("closed"); } }
var [first] = closable();
assert.sameValue(first, 1);
assert.sameValue(log.join(), "closed");

// Iterator helpers pull from generators lazily.
assert.sameValue(Array.from(plain("h", 5).take(2)).join(), "h0,h1");

// Generators whose bodies keep the state machine behave identically.
function* viaStar(n) { yield* [1, 2, 3].slice(0, n); return "end"; }
assert.sameValue(drain(viaStar(2)), "Y:1,Y:2,R:end");
function* defaultYield() { var [a = yield "need a"] = []; return "a=" + a; }
assert.sameValue(drain(defaultYield(), [0, "A"]), "Y:need a,R:a=A");
function* templated() { return `<${yield "x"}>`; }
assert.sameValue(drain(templated(), [0, "T"]), "Y:x,R:<T>");

// A state-machine generator iterating a suspending-frame generator, and back.
function* viaStarOfFrame() { for (const v of plain("m", 2)) yield* [v, v + "'"]; }
assert.sameValue(drain(viaStarOfFrame()), "Y:m0,Y:m0',Y:m1,Y:m1',R:undefined");

// Mutual recursion between the two kinds.
function* even(n) { if (n === 0) return "even"; return yield* odd(n - 1); }
function* odd(n) { if (n === 0) return "odd"; yield n; return yield* even(n - 1); }
assert.sameValue(drain(even(3)), "Y:2,R:odd");

// A generator object protocol: prototype chain, toStringTag, iterability.
function* proto() {}
var g = proto();
assert.sameValue(Object.getPrototypeOf(g), proto.prototype);
assert.sameValue(Object.prototype.toString.call(g), "[object Generator]");
assert.sameValue(g[Symbol.iterator](), g);
assert.sameValue(typeof g.next, "function");
assert.sameValue(Object.getPrototypeOf(proto.prototype), Object.getPrototypeOf(function*(){}).prototype);

// Replacing prototype.next after creation is visible (generic iteration).
function* patched() { yield 1; yield 2; }
var gp = patched();
var originalNext = Object.getPrototypeOf(Object.getPrototypeOf(gp)).next;
var calls = 0;
var proxyNext = function(v) { calls++; return originalNext.call(this, v); };
Object.getPrototypeOf(Object.getPrototypeOf(gp)).next = proxyNext;
try {
  assert.sameValue([...gp].join(), "1,2");
  assert.sameValue(calls, 3);
} finally {
  Object.getPrototypeOf(Object.getPrototypeOf(gp)).next = originalNext;
}
