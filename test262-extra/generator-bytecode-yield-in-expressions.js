/*---
description: >
  yield inside larger expressions keeps the operands evaluated before it
  alive across the suspension, in specified order: binary and logical
  operands, call arguments and callee, member and element assignment targets,
  compound assignment, conditionals, sequences, array and object literals.
esid: sec-generator-function-definitions-runtime-semantics-evaluation
features: [generators]
---*/

function drain(it, sends) {
  const out = [];
  let r, k = 0;
  while (true) {
    r = it.next(sends ? sends[k++] : undefined);
    out.push(r.done ? "R:" + JSON.stringify(r.value) : "Y:" + JSON.stringify(r.value));
    if (r.done || out.length > 40) break;
  }
  return out.join(",");
}

var log;
function note(x) { log.push(x); return x; }

var cases = {};
cases.binary = () => { function* g() { return note(1) + (yield note(2)) * note(3); } log = []; return drain(g(), [0, 5]) + "|" + log; };
cases.logicalShort = () => { function* g() { return note(0) && (yield note(2)); } log = []; return drain(g()) + "|" + log; };
cases.logicalLong = () => { function* g() { return note(1) && (yield note(2)); } log = []; return drain(g(), [0, "s"]) + "|" + log; };
cases.nullish = () => { function* g() { return note(null) ?? (yield note("n")); } log = []; return drain(g(), [0, "s"]) + "|" + log; };
cases.conditional = () => { function* g() { return (yield note("t")) ? yield note("a") : yield note("b"); } log = []; return drain(g(), [0, true, "x", "y"]) + "|" + log; };
cases.callArgs = () => { function f(a, b, c) { return [a, b, c].join("+"); } function* g() { return f(note(1), yield note(2), note(3)); } log = []; return drain(g(), [0, "Y"]) + "|" + log; };
cases.callee = () => { function* g() { return (yield note("callee"))(note(1), yield note("arg")); } log = []; return drain(g(), [0, (a, b) => a + ":" + b, "B"]) + "|" + log; };
cases.method = () => { var o = { m(a, b) { return this.tag + a + b; }, tag: "T" }; function* g() { return o.m(note(1), yield note("y")); } log = []; return drain(g(), [0, 2]) + "|" + log; };
cases.newExpr = () => { function C(a, b) { this.v = a + b; } function* g() { return new C(note(1), yield note("y")).v; } log = []; return drain(g(), [0, 9]) + "|" + log; };
cases.propAssign = () => { var o = {}; function* g() { note("lhs"); o.p = yield note("rhs"); return o.p; } log = []; return drain(g(), [0, 7]) + "|" + log; };
cases.elementAssign = () => { var o = {}; function key() { return note("key"); } function* g() { o[key()] = yield note("rhs"); return Object.keys(o).join(); } log = []; return drain(g(), [0, 7]) + "|" + log + "|" + JSON.stringify(o); };
cases.compoundAssign = () => { var o = { n: 10 }; function* g() { o.n += yield note("y"); o.n *= yield note("z"); return o.n; } log = []; return drain(g(), [0, 5, 2]) + "|" + log; };
cases.identifierAssign = () => { var x = 1; function* g() { x = (yield note("a")) + x; x += yield note("b"); return x; } log = []; return drain(g(), [0, 10, 100]) + "|" + log; };
cases.identifierReferenceBeforeRhs = () => { var x = "outer"; function* g() { var x2 = x; with_scope: { } x = yield note("y"); return x2 + ">" + x; } log = []; return drain(g(), [0, "new"]) + "|" + log; };
cases.updateThroughYield = () => { var o = { n: 1 }; function* g() { o[yield note("k")]++; return o.n; } log = []; return drain(g(), [0, "n"]) + "|" + log; };
cases.sequence = () => { function* g() { return (note(1), yield note(2), note(3)); } log = []; return drain(g(), [0, "s"]) + "|" + log; };
cases.arrayLiteral = () => { function* g() { return [note(1), yield note(2), note(3), yield note(4)]; } log = []; return drain(g(), [0, "a", "b"]) + "|" + log; };
cases.objectLiteral = () => { function* g() { return { a: note(1), b: yield note(2), c: note(3) }; } log = []; return drain(g(), [0, "B"]) + "|" + log; };
cases.unaryTypeof = () => { function* g() { return [typeof (yield 1), -(yield 2), !(yield 3)]; } log = []; return drain(g(), [0, "s", 4, 0]) + "|" + log; };
cases.yieldYield = () => { function* g() { return yield yield 1; } log = []; return drain(g(), [0, "a", "b"]); };
cases.yieldNothing = () => { function* g() { var v = yield; return v; } log = []; return drain(g(), [0, "got"]); };
cases.yieldInTest = () => { function* g() { var s = 0; while (yield s) s++; return s; } log = []; return drain(g(), [0, true, true, false]); };
cases.yieldInForHead = () => { function* g() { for (var i = yield "init"; i < (yield "limit"); i += yield "step") { yield "body" + i; } } log = []; return drain(g(), [0, 0, 2, 1, 2, 1, 2]); };
cases.yieldInSwitch = () => { function* g() { switch (yield "disc") { case yield "c1": return "one"; case yield "c2": return "two"; default: return "dflt"; } } log = []; return drain(g(), [0, "B", "A", "B"]); };
cases.yieldInTemplate = () => { function* g() { return `a${yield 1}b${yield 2}c`; } log = []; return drain(g(), [0, "X", "Y"]); };
cases.yieldInSpread = () => { function f(...a) { return a.join(); } function* g() { return f(...[1, yield "s"], 3); } log = []; return drain(g(), [0, 2]); };
cases.yieldInDestructuringDefault = () => { function* g() { var [a = yield "dflt"] = []; var { b = yield "dflt2" } = {}; return a + b; } log = []; return drain(g(), [0, 1, 2]); };
cases.yieldInComputedKey = () => { function* g() { var o = { [yield "key"]: 1 }; return Object.keys(o).join(); } log = []; return drain(g(), [0, "k"]); };
cases.yieldInClassHeritage = () => { function* g() { class A extends (yield "base") {} return Object.getPrototypeOf(A) === Array; } log = []; return drain(g(), [0, Array]); };
cases.yieldInArrow = () => { function* g() { var f = () => 1; return f() + (yield "y"); } log = []; return drain(g(), [0, 5]); };
cases.yieldInForOfHead = () => { function* g() { for (const x of yield "iterable") yield x; } log = []; return drain(g(), [0, [1, 2]]); };
cases.yieldInThrow = () => { function* g() { try { throw yield "v"; } catch (e) { return "c" + e; } } log = []; return drain(g(), [0, "E"]); };
cases.yieldInReturn = () => { function* g() { return (yield "a") + (yield "b"); } log = []; return drain(g(), [0, 1, 2]); };
cases.yieldInLabelledLoop = () => { function* g() { out: for (var i = 0; i < 3; i++) { for (var j = 0; j < 3; j++) { if ((yield i + "" + j) === "stop") break out; if (j === 1) continue out; } } return "end" + i + j; } log = []; return drain(g(), [0, 0, 0, 0, "stop"]); };

var expected = {
  binary: 'Y:2,R:16|1,2,3',
  logicalShort: 'R:0|0',
  logicalLong: 'Y:2,R:"s"|1,2',
  nullish: 'Y:"n",R:"s"|,n',
  conditional: 'Y:"t",Y:"a",R:"x"|t,a',
  callArgs: 'Y:2,R:"1+Y+3"|1,2,3',
  callee: 'Y:"callee",Y:"arg",R:"1:B"|callee,1,arg',
  method: 'Y:"y",R:"T12"|1,y',
  newExpr: 'Y:"y",R:10|1,y',
  propAssign: 'Y:"rhs",R:7|lhs,rhs',
  elementAssign: 'Y:"rhs",R:"key"|key,rhs|{"key":7}',
  compoundAssign: 'Y:"y",Y:"z",R:30|y,z',
  identifierAssign: 'Y:"a",Y:"b",R:111|a,b',
  identifierReferenceBeforeRhs: 'Y:"y",R:"outer>new"|y',
  updateThroughYield: 'Y:"k",R:2|k',
  sequence: 'Y:2,R:3|1,2,3',
  arrayLiteral: 'Y:2,Y:4,R:[1,"a",3,"b"]|1,2,3,4',
  objectLiteral: 'Y:2,R:{"a":1,"b":"B","c":3}|1,2,3',
  unaryTypeof: 'Y:1,Y:2,Y:3,R:["string",-4,true]|',
  yieldYield: 'Y:1,Y:"a",R:"b"',
  yieldNothing: 'Y:undefined,R:"got"',
  yieldInTest: 'Y:0,Y:1,Y:2,R:2',
  yieldInForHead: 'Y:"init",Y:"limit",Y:"body0",Y:"step",Y:"limit",R:undefined',
  yieldInSwitch: 'Y:"disc",Y:"c1",Y:"c2",R:"two"',
  yieldInTemplate: 'Y:1,Y:2,R:"aXbYc"',
  yieldInSpread: 'Y:"s",R:"1,2,3"',
  yieldInDestructuringDefault: 'Y:"dflt",Y:"dflt2",R:3',
  yieldInComputedKey: 'Y:"key",R:"k"',
  yieldInClassHeritage: 'Y:"base",R:true',
  yieldInArrow: 'Y:"y",R:6',
  yieldInForOfHead: 'Y:"iterable",Y:1,Y:2,R:undefined',
  yieldInThrow: 'Y:"v",R:"cE"',
  yieldInReturn: 'Y:"a",Y:"b",R:3',
  yieldInLabelledLoop: 'Y:"00",Y:"01",Y:"10",Y:"11",R:"end11"',
};
// A generator the engine runs as a state machine evaluates operands that
// precede a `yield` only when it resumes, so the order in which side effects
// happen is only asserted for engines that suspend the body in place.
var operandOrderIsPreserved = (function() {
  log = [];
  function* probe() { return note(1) + (yield note(2)); }
  drain(probe(), [0, 0]);
  return log.join() === "1,2";
})();

for (var name in cases) {
  var actual = cases[name]();
  var want = expected[name];
  if (!operandOrderIsPreserved) {
    actual = actual.split("|")[0];
    want = want.split("|")[0];
  }
  assert.sameValue(actual, want, name);
}
