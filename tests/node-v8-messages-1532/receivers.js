// V8 text of builtins called on the wrong receiver (#1532), run verbatim on
// Node and on `jsse --node` by scripts/run-node-v8-messages-1532-oracle.sh.
// Every line is `Class.prototype.method [receiver] -> TypeError: message`.
const TypedArray = Object.getPrototypeOf(Uint8Array);
const groups = [
  ["String", String, ["at", "trim", "trimStart", "trimEnd", "includes", "padEnd", "repeat", "split", "toUpperCase"]],
  ["Array", Array, ["concat", "every", "filter", "find", "findIndex", "findLast", "findLastIndex", "forEach", "indexOf", "map", "reduce", "reduceRight", "some", "join", "includes", "at", "slice"]],
  ["Object", Object, ["toLocaleString", "hasOwnProperty"]],
  ["RegExp", RegExp, ["exec", "test", "compile", "toString"]],
  ["Date", Date, ["getTime", "setHours", "setTime", "toString", "toISOString", "toDateString", "toLocaleString", "toUTCString"]],
  ["DataView", DataView, ["getInt8", "getUint32", "getFloat64", "setInt16", "setBigInt64"]],
  ["ArrayBuffer", ArrayBuffer, ["slice", "resize", "transfer", "transferToFixedLength"]],
  ["Promise", Promise, ["then", "catch"]],
  ["%TypedArray%", TypedArray, ["at", "join", "slice", "sort", "subarray", "toSorted", "fill", "map", "forEach", "set"]],
];
const receivers = [null, 1, {}];
const lines = [];
for (const [label, ctor, names] of groups) {
  for (const name of names) {
    const fn = ctor.prototype[name];
    for (const receiver of receivers) {
      let text;
      try {
        fn.call(receiver, () => 1);
        text = "ok";
      } catch (e) {
        text = `${e.name}: ${e.message}`;
      }
      lines.push(`${label}.prototype.${name} [${String(receiver)}] -> ${text}`);
    }
  }
}
for (const name of ["all", "allSettled", "any", "race"]) {
  for (const receiver of [1, "abc", undefined]) {
    try {
      Promise[name].call(receiver, []);
    } catch (e) {
      lines.push(`Promise.${name} [${String(receiver)}] -> ${e.name}: ${e.message}`);
    }
  }
}
const symbolMethods = ["match", "matchAll", "replace", "search", "split"];
for (const name of symbolMethods) {
  for (const receiver of [1, "abc", {}]) {
    try {
      RegExp.prototype[Symbol[name]].call(receiver, "a");
    } catch (e) {
      lines.push(`RegExp.prototype[@@${name}] [${String(receiver)}] -> ${e.name}: ${e.message}`);
    }
  }
}
for (const [label, fn, args] of [
  ["Function.prototype.call.call", Function.prototype.call, [1]],
  ["Function.prototype.call.call", Function.prototype.call, [{}]],
  ["Function.prototype.apply.call", Function.prototype.apply, [1]],
  ["BigInt.asIntN", BigInt.asIntN, [-1, 1n]],
  ["BigInt.asUintN", BigInt.asUintN, [2 ** 53, 1n]],
]) {
  try {
    fn.call(label.endsWith(".call") ? args[0] : undefined, ...(label.endsWith(".call") ? [] : args));
  } catch (e) {
    lines.push(`${label}(${String(args[0])}) -> ${e.name}: ${e.message}`);
  }
}
lines.sort();
console.log(lines.join("\n"));
