// seeded random value trees inspected under random options
var leaves = [0, -0, 1, 42, 3.14, -7, 1e21, NaN, Infinity, "", "s", "a b", "it's", 'q"', "multi\nline", "x".repeat(40), "é", true, false, null, undefined,
  5n, Symbol("r"), function leaf() {}, () => {}, class LC {}, new Date(0), /r+/gi, new Error("x"), new Number(2), new String("bx")];
var optionSets = [undefined, { depth: 0 }, { depth: 1 }, { depth: null }, { compact: false }, { compact: 1 }, { compact: 5 }, { breakLength: 30 },
  { breakLength: Infinity }, { sorted: true }, { maxArrayLength: 2 }, { maxStringLength: 5 }, { numericSeparator: true },
  { colors: true }, { getters: true }, { depth: 1, compact: false, breakLength: 40 }, { sorted: true, maxArrayLength: 3, depth: 4 }];
function fixErrors(v, seen) {
  if (v instanceof Error && !v.__fixed) {
    v.stack = v.name + ": " + v.message + "\n    at fake (/app/file.js:1:1)";
    v.__fixed = true;
    Object.defineProperty(v, "__fixed", { enumerable: false });
  }
  return v;
}
function randomValue(depth) {
  var k = depth <= 0 ? 0 : rnd(10);
  if (k < 3) return fixErrors(pick(leaves));
  if (k === 3 || k === 4) {
    var a = [];
    var n = rnd(depth > 2 ? 9 : 5);
    for (var i = 0; i < n; i++) a.push(randomValue(depth - 1));
    if (rnd(8) === 0) a.extra = randomValue(depth - 1);
    if (rnd(8) === 0) delete a[rnd(n + 1)];
    return a;
  }
  if (k === 5 || k === 6) {
    var o = {};
    var m = rnd(depth > 2 ? 8 : 4);
    for (var j = 0; j < m; j++) o[pick(["a", "b", "key", "k-2", "x y", "n1", "z", "é", "$"]) + j] = randomValue(depth - 1);
    if (rnd(10) === 0) o[Symbol("rs")] = randomValue(depth - 1);
    if (rnd(10) === 0) Object.defineProperty(o, "hid", { value: randomValue(depth - 1), enumerable: false });
    if (rnd(10) === 0) Object.defineProperty(o, "acc", { get: function () { return 1; }, set: function (v) {}, enumerable: true });
    if (rnd(12) === 0) return Object.setPrototypeOf(o, null);
    if (rnd(12) === 0) {
      var C = function RandomClass() {};
      return Object.setPrototypeOf(o, C.prototype);
    }
    return o;
  }
  if (k === 7) {
    var map = new Map();
    for (var p = rnd(4); p > 0; p--) map.set(randomValue(depth - 2), randomValue(depth - 1));
    return map;
  }
  if (k === 8) {
    var set = new Set();
    for (var q = rnd(4); q > 0; q--) set.add(randomValue(depth - 2));
    return set;
  }
  return pick([new Uint8Array(rnd(6)), new Float32Array([1.5, 2]), new ArrayBuffer(rnd(5)), Promise.resolve(randomValue(depth - 2)), new WeakSet(), new Proxy({ p: 1 }, {})]);
}
for (var t = 0; t < 250; t++) {
  var value = randomValue(4);
  show("rand" + t, value, pick(optionSets));
  if (t % 5 === 0) fmt("rfmt" + t, pick(["%s", "%O", "%j", "x %s y", "%d"]), value, randomValue(2));
}
