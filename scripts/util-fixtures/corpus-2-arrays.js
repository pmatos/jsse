// arrays, typed arrays, buffers
var sparse = [1, , 3];
var extra = [1, 2];
extra.foo = "bar";
extra[Symbol("s")] = 1;
var holesEnd = [1, 2, , , ,];
var big = [];
for (var bi = 0; bi < 150; bi++) big.push(bi);
var strs = [];
for (var si = 0; si < 40; si++) strs.push("item-" + si * 7);
var mixed = [];
for (var mi = 0; mi < 30; mi++) mixed.push(mi % 3 === 0 ? "s" + mi : mi % 3 === 1 ? mi * 1000 : { n: mi });
var decimals = [];
for (var di = 0; di < 27; di++) decimals.push(di * 1.5 - 7);
var nested = [[1, [2, [3, [4, [5]]]]]];
var bigints = [1n, 22n, 333n, 4444n, 55555n, 666666n, 7777777n];
showAll("arr", [[], [1], [1, 2, 3], sparse, extra, holesEnd, big, strs, mixed, decimals, nested, bigints,
  [undefined, null], [[]], [{}], [[], []], new Array(5), new Array(120), Array.from({ length: 7 }, function (_, i) { return i * i; })]);
showAll("arr-opt", [big, strs, nested, extra, mixed], { maxArrayLength: 5 });
showAll("arr-opt0", [big, [1]], { maxArrayLength: 0 });
showAll("arr-null", [big], { maxArrayLength: null, breakLength: 120 });
showAll("arr-depth", [nested], { depth: 1 });
showAll("arr-depth0", [nested, [[]], [{}]], { depth: 0 });
showAll("arr-compact", [big.slice(0, 12), strs.slice(0, 8), mixed.slice(0, 7)], { compact: false });
showAll("arr-compact1", [{ a: [1, 2, { b: 1 }], c: "x" }, mixed.slice(0, 7)], { compact: 1 });
showAll("arr-break", [strs, decimals], { breakLength: 40 });
showAll("arr-break-inf", [strs, nested, { a: 1, b: { c: [1, 2, 3] } }], { breakLength: Infinity });
showAll("arr-sorted", [["b", "a", "c"], { z: 1, a: 2, m: [3, 1, 2] }, new Set([3, 1, 2]), new Map([["b", 1], ["a", 2]])], { sorted: true });
showAll("arr-sortfn", [{ z: 1, a: 2, m: 3 }], { sorted: function (a, b) { return a < b ? 1 : -1; } });
showAll("arr-hidden", [[1, 2], extra, new Array(3)], { showHidden: true });
// typed arrays
var kinds = [Int8Array, Uint8Array, Uint8ClampedArray, Int16Array, Uint16Array, Int32Array, Uint32Array, Float32Array, Float64Array];
kinds.forEach(function (K) {
  show("ta-" + K.name, new K([1, 2, 3, 120]));
  show("ta-empty-" + K.name, new K(0));
});
show("ta-big64", new BigInt64Array([1n, -2n]));
show("ta-ubig64", new BigUint64Array([1n, 2n ** 64n - 1n]));
show("ta-float", new Float64Array([0.1, -0, NaN, Infinity, 1e21]));
show("ta-long", new Uint8Array(120));
show("ta-long-opt", new Uint8Array(120), { maxArrayLength: 3 });
show("ta-hidden", new Uint8Array([1, 2]), { showHidden: true });
show("ta-sub", new Uint8Array([1, 2, 3, 4, 5]).subarray(1, 3));
var tae = new Uint8Array([1, 2]);
tae.extra = "yes";
show("ta-extra", tae);
show("ta-nested", { t: new Int16Array([1, -1]), u: [new Float32Array(2)] });
show("ta-sep", new Uint32Array([1234567, 8901234]), { numericSeparator: true });
show("ta-colors", new Uint8Array([1, 2]), { colors: true });
class MyTA extends Uint8Array {}
show("ta-sub-class", new MyTA([1, 2]));
// array buffers
show("ab", new ArrayBuffer(4));
show("ab-data", new Uint8Array([1, 2, 255, 16]).buffer);
show("ab-big", new ArrayBuffer(100));
show("ab-big-opt", new ArrayBuffer(100), { maxArrayLength: 4 });
show("ab-hidden", new ArrayBuffer(3), { showHidden: true });
show("ab-empty", new ArrayBuffer(0));
show("sab", new SharedArrayBuffer(4));
show("dv", new DataView(new ArrayBuffer(4)));
show("dv-off", new DataView(new ArrayBuffer(8), 2, 4));
show("dv-hidden", new DataView(new ArrayBuffer(4), 1), { showHidden: true });
show("ab-nested", { buf: new ArrayBuffer(2), view: new DataView(new ArrayBuffer(2)) });
var det = new ArrayBuffer(4);
if (typeof det.transfer === "function") { det.transfer(); show("ab-detached", det); }
// array-likes and subclasses
class MyArray extends Array {}
show("arr-sub", MyArray.from([1, 2, 3]));
show("arr-sub-empty", new MyArray());
var an = [1, 2];
Object.setPrototypeOf(an, null);
show("arr-nullproto", an);
(function () { show("arguments", arguments); })(1, "a", {});
(function () { show("arguments-empty", arguments); })();
var arrLike = { 0: "a", 1: "b", length: 2 };
show("arraylike", arrLike);
show("arr-tag", Object.assign([1], { [Symbol.toStringTag]: "Tagged" }));
show("arr-neg", Object.assign([1], { "-1": 2, "1.5": 3 }));
show("arr-longstr", [["a".repeat(80), "b".repeat(80)]]);
show("arr-objs", Array.from({ length: 8 }, function (_, i) { return { id: i, name: "n" + i }; }));
show("arr-2d", Array.from({ length: 4 }, function (_, i) { return Array.from({ length: 4 }, function (_, j) { return i * j; }); }));
show("arr-wide", Array.from({ length: 101 }, function (_, i) { return "v" + i; }));
show("arr-pad-nums", [1, 22, 333, 4444, 55555, 666666, 7777777, 88888888, 1, 22, 333, 4444, 55555, 666666, 7777777]);
show("arr-pad-neg", [-1, 22, -333, 4444, -55555, 666666, -7777777, 88888888]);
show("arr-unicode", ["中文", "abc", "\u{1f600}", "é", "é", "x", "yy", "zzz"]);
show("arr-empty-str", ["", "", "", "", "", "", "", ""]);
show("arr-bool", [true, false, true, false, true, false, true, false]);
show("arr-undef", [undefined, undefined, null, undefined, null, undefined, null, undefined]);
for (var ri = 0; ri < 40; ri++) {
  var len = rnd(40);
  var arr = [];
  for (var rj = 0; rj < len; rj++) {
    var k = rnd(7);
    arr.push(k === 0 ? rnd(100000) : k === 1 ? "s" + rnd(1000) : k === 2 ? rnd(2) === 0 : k === 3 ? null : k === 4 ? -rnd(100) / 4 : k === 5 ? BigInt(rnd(1000)) : "x".repeat(rnd(20)));
  }
  show("arr-rand" + ri, arr);
}
