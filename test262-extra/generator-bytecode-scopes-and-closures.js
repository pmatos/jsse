/*---
description: >
  Lexical environments survive suspension: per-iteration bindings of for
  loops captured by closures, block-scoped let/const/class/function
  declarations, catch parameters, switch case scopes, TDZ, and labelled
  break/continue across yields (sec-createperiterationenvironment,
  sec-blockdeclarationinstantiation).
esid: sec-generator-function-definitions-runtime-semantics-evaluation
flags: [noStrict]
features: [generators]
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

// Closures capture a fresh binding per iteration even across yields.
function* perIteration() {
  var fns = [];
  for (let i = 0; i < 3; i++) {
    fns.push(() => i);
    yield i;
    i += 0;
  }
  yield fns.map(f => f()).join("");
}
assert.sameValue(drain(perIteration()), "Y:0,Y:1,Y:2,Y:012,R:undefined");

// The loop body mutates the per-iteration binding after the yield.
function* mutateAfterYield() {
  var fns = [];
  for (let i = 0; i < 3; i++) {
    fns.push(() => i);
    yield "a" + i;
    i++;
  }
  yield fns.map(f => f()).join();
}
assert.sameValue(drain(mutateAfterYield()), "Y:a0,Y:a2,Y:1,3,R:undefined");

// let in a while body is a new binding each time round.
function* whileBody() {
  var fns = [], n = 0;
  while (n < 3) {
    let captured = n * 10;
    fns.push(() => captured);
    yield n;
    n++;
  }
  return fns.map(f => f()).join();
}
assert.sameValue(drain(whileBody()), "Y:0,Y:1,Y:2,R:0,10,20");

// TDZ for a lexical declared after a yield in the same block.
function* tdz() {
  {
    try { later; } catch (e) { yield e.constructor.name; }
    yield "before";
    let later = 1;
    yield later;
  }
}
assert.sameValue(drain(tdz()), "Y:ReferenceError,Y:before,Y:1,R:undefined");

// Top-level lexicals of the generator body exist (in TDZ) from the call.
function* topLevelTdz() {
  yield typeof outerRead();
  let value = 5;
  function outerRead() { return value; }
  yield outerRead();
}
var g = topLevelTdz();
assert.throws(ReferenceError, function() { g.next(); });

// Function declarations: hoisted in the body, and block-level ones in blocks.
function* hoisting() {
  yield hoisted();
  { function inner() { return "inner"; } yield inner(); }
  function hoisted() { return "hoisted"; }
  yield typeof inner;
}
assert.sameValue(drain(hoisting()), "Y:hoisted,Y:inner,Y:function,R:undefined");

// Block scopes shadow and restore across suspension.
function* shadowing() {
  let x = "outer";
  {
    let x = "inner";
    yield x;
    {
      const x = "innermost";
      yield x;
    }
    yield x;
  }
  yield x;
}
assert.sameValue(drain(shadowing()), "Y:inner,Y:innermost,Y:inner,Y:outer,R:undefined");

// Class declarations keep their binding.
function* classes() {
  class A { static tag() { return "A"; } }
  yield A.tag();
  { class A { static tag() { return "inner"; } } yield A.tag(); }
  yield A.tag();
}
assert.sameValue(drain(classes()), "Y:A,Y:inner,Y:A,R:undefined");

// catch parameter scopes and a yield in the catch body.
function* catchScope() {
  var e = "outer-e";
  try { throw "thrown"; }
  catch (e) { yield e; let local = e + "!"; yield local; }
  yield e;
}
assert.sameValue(drain(catchScope()), "Y:thrown,Y:thrown!,Y:outer-e,R:undefined");

// Switch case bodies share one scope that spans the yields.
function* switchScope(v) {
  switch (v) {
    case 1:
      let shared = "s";
      yield shared;
    case 2:
      yield "fall";
      shared = "changed";
      yield shared;
      break;
    default:
      yield "default";
  }
  yield "end";
}
assert.sameValue(drain(switchScope(1)), "Y:s,Y:fall,Y:changed,Y:end,R:undefined");
assert.sameValue(drain(switchScope(3)), "Y:default,Y:end,R:undefined");

// Labelled break and continue across yields leave scopes correctly.
function* labels() {
  outer: for (let i = 0; i < 3; i++) {
    let seen = i;
    for (let j = 0; j < 3; j++) {
      let both = seen + ":" + j;
      yield both;
      if (j === 1) continue outer;
      if (i === 2) break outer;
    }
  }
  yield "done";
}
assert.sameValue(drain(labels()), "Y:0:0,Y:0:1,Y:1:0,Y:1:1,Y:2:0,Y:done,R:undefined");

// for-in and for-of loops: enumerators and iterators survive suspension.
function* loops() {
  for (const k in { a: 1, b: 2 }) yield "in:" + k;
  for (const v of [10, 20]) yield "of:" + v;
  for (var w of new Set([1, 2])) yield "set:" + w;
  for (const [a, b] of [[1, 2], [3, 4]]) yield "pair:" + a + b;
  for (const { x, y = 7 } of [{ x: 1 }, { x: 2, y: 3 }]) yield "obj:" + x + y;
}
assert.sameValue(
  drain(loops()),
  "Y:in:a,Y:in:b,Y:of:10,Y:of:20,Y:set:1,Y:set:2,Y:pair:12,Y:pair:34,Y:obj:17,Y:obj:23,R:undefined"
);

// The enumeration order of for-in follows live modification after a yield.
function* forInMutation() {
  var o = { a: 1, b: 2, c: 3 };
  for (var k in o) { yield k; delete o.c; }
}
assert.sameValue(drain(forInMutation()), "Y:a,Y:b,R:undefined");

// do-while and nested loops with continue.
function* doWhile() {
  var i = 0;
  do { i++; if (i % 2) continue; yield i; } while (i < 6);
  return "end";
}
assert.sameValue(drain(doWhile()), "Y:2,Y:4,Y:6,R:end");

// Variables: var hoisting and parameter scope.
function* vars(a, b = () => a) {
  var a;
  yield a;
  a = "changed";
  yield b();
  yield a;
}
assert.sameValue(drain(vars("p")), "Y:p,Y:p,Y:changed,R:undefined");

// A generator method keeps the object's other properties reachable via this.
var obj = { tag: "T", *g() { yield this.tag; yield this === obj; } };
assert.sameValue(drain(obj.g()), "Y:T,Y:true,R:undefined");

// Arrow functions defined in a generator see its this and arguments.
function* arrows() {
  var f = () => [this.tag, arguments.length].join();
  yield f();
  yield [1, 2].map(x => x * arguments.length).join();
}
assert.sameValue(drain(arrows.call({ tag: "X" }, 1, 2, 3)), "Y:X,3,Y:3,6,R:undefined");

// Recursive generators through yield* of another generator function.
function* tree(n) { if (n > 0) { yield* tree(n - 1); yield n; yield* tree(n - 1); } }
assert.sameValue(drain(tree(2)), "Y:1,Y:2,Y:1,R:undefined");

// Many live generators keep independent frames.
var gens = [];
function* indexed(i) { var x = i; while (true) { x += yield x; } }
for (var n = 0; n < 50; n++) { gens.push(indexed(n)); gens[n].next(); }
for (var round = 0; round < 3; round++) for (n = 0; n < 50; n++) gens[n].next(1);
assert.sameValue(gens.map(g => g.next(0).value).join(","), Array.from({ length: 50 }, (_, i) => i + 3).join(","));
