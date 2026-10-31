// V8 text of symbol-keyed builtins on the wrong receiver, a non-callable
// `@@iterator` handed to a typed array, `Promise.prototype.finally` on a
// non-promise, and an out-of-range `Array.prototype.with` (#1544), run
// verbatim on Node and on `jsse --node`.
function run(name, fn) {
  try {
    fn();
    console.log(name, "-> ok");
  } catch (e) {
    console.log(name, "->", `${e.name}: ${e.message}`);
  }
}

run("String[@@iterator] null", () => String.prototype[Symbol.iterator].call(null));
run("String[@@iterator] undefined", () => String.prototype[Symbol.iterator].call(undefined));
run("Date[@@toPrimitive] 1", () => Date.prototype[Symbol.toPrimitive].call(1));
run("Date[@@toPrimitive] undefined", () => Date.prototype[Symbol.toPrimitive].call(undefined, "number"));
run("Date[@@toPrimitive] hint", () => Date.prototype[Symbol.toPrimitive].call(new Date(0), "x"));
run("Date[@@toPrimitive] hint number", () => Date.prototype[Symbol.toPrimitive].call({}, 1));
run("Date[@@toPrimitive] no hint", () => Date.prototype[Symbol.toPrimitive].call({}));
run("Symbol[@@toPrimitive] 1", () => Symbol.prototype[Symbol.toPrimitive].call(1));
run("Symbol[@@toPrimitive] object", () => Symbol.prototype[Symbol.toPrimitive].call({}));

for (const value of [1, true, undefined, null, "abc", Symbol("s"), {}, function () {}]) {
  run(`finally ${String(typeof value)}`, () => Promise.prototype.finally.call(value, () => 1));
}
run("finally without callback", () => Promise.prototype.finally.call(1));

for (const iterator of [1, {}, true, 0, "x"]) {
  run(`new Uint8Array iterator ${typeof iterator}`, () => new Uint8Array({ [Symbol.iterator]: iterator }));
  run(`Uint8Array.from iterator ${typeof iterator}`, () => Uint8Array.from({ [Symbol.iterator]: iterator }));
}
run("new BigInt64Array iterator", () => new BigInt64Array({ [Symbol.iterator]: 1 }));
run("new Uint8Array null iterator", () => new Uint8Array({ [Symbol.iterator]: null, length: 1 }));

for (const index of [-2, 5, 1.5, 5.5, Infinity, -Infinity, "7", 2 ** 53, 1e21, true, null, {}, { valueOf() { return 9; } }]) {
  run(`[1].with(${typeof index === "object" ? "object" : String(index)})`, () => [1].with(index, 1));
}
run("[].with()", () => [].with());
run("[].with(undefined)", () => [].with(undefined, 1));
run("array-like with", () => Array.prototype.with.call({ length: 2 }, 3, 1));
