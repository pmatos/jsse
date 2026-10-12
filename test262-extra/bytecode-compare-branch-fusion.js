/*---
description: >
  A comparison used directly as a branch condition (if, while, for, do-while,
  conditional expression) behaves like the same comparison evaluated as a
  value: same boolean, same ToPrimitive order (including LeftFirst=false for
  `>` and `<=`) and same abrupt completions, for numbers, NaN, strings,
  BigInts, nullish values and objects.
info: |
  sec-relational-operators, sec-islessthan, sec-islooselyequal,
  sec-isstrictlyequal, sec-if-statement-runtime-semantics-evaluation,
  sec-conditional-operator-runtime-semantics-evaluation.
esid: sec-islessthan
---*/

var log = [];

function mk(tag, value) {
  return { valueOf: function () { log.push(tag); return value; } };
}

var operandSets = [
  [1, 2], [2, 1], [1, 1], [NaN, 1], [1, NaN], [NaN, NaN], [-0, 0], ["10", 9],
  ["a", "b"], ["b", "a"], ["2", "10"], [1n, 2], [2, 1n], [1n, 1], [null, undefined],
  [null, 0], [undefined, 0], [true, 1], [Infinity, Infinity],
];
var objectSets = [
  function () { return [mk("l", 1), mk("r", 2)]; },
  function () { return [mk("l", 2), mk("r", 2)]; },
  function () { return [mk("l", 3), 1]; },
  function () { return [1, mk("r", 3)]; },
  function () { return [mk("l", "x"), mk("r", "x")]; },
];

var ops = ["<", ">", "<=", ">=", "==", "!=", "===", "!=="];

// Each shape runs on the bytecode VM; the `generic` twin contains an object
// literal so it is evaluated by the tree-walker.
var shapes = {
  ifElse: "if (a OP b) { return 1; } else { return 0; }",
  whileLoop: "var n = 0; while (a OP b) { n++; if (n > 2) { break; } } return n;",
  forLoop: "var n = 0; for (var i = 0; a OP b; i++) { n++; if (n > 2) { break; } } return n;",
  doWhile: "var n = 0; do { n++; if (n > 2) { break; } } while (a OP b); return n;",
  conditional: "return (a OP b) ? 1 : 0;",
  value: "var t = a OP b; return t ? 1 : 0;",
};

function outcome(fn, pair) {
  log.length = 0;
  var result;
  try {
    result = fn(pair[0], pair[1]);
  } catch (e) {
    result = "throw:" + e.constructor.name;
  }
  return result + "|" + log.join("");
}

Object.keys(shapes).forEach(function (shape) {
  ops.forEach(function (op) {
    var body = shapes[shape].replace(/OP/g, op);
    var compiled = new Function("a", "b", body);
    var generic = new Function("a", "b", "({}); " + body);
    operandSets.forEach(function (pair) {
      assert.sameValue(outcome(compiled, pair), outcome(generic, pair),
        shape + " " + String(pair[0]) + " " + op + " " + String(pair[1]));
    });
    objectSets.forEach(function (make, i) {
      assert.sameValue(outcome(compiled, make()), outcome(generic, make()),
        shape + " object set " + i + " " + op);
    });
  });
});

function nanBranches() {
  var n = 0;
  if (NaN < 1) { n += 1; }
  if (!(NaN >= 1)) { n += 10; }
  if (NaN != NaN) { n += 100; }
  if (1 < 2) { n += 1000; }
  return n;
}
assert.sameValue(nanBranches(), 1110);
