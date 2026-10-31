// V8 message text of builtin errors by family (#1510): Reflect, RegExp getters and typed array constructors.
// Run verbatim on Node and on `jsse --node` by
// scripts/run-node-v8-messages-1510-oracle.sh.
const out = [];
function t(label, f) {
  try {
    f();
    out.push(label + " => no error");
  } catch (e) {
    out.push(label + " => " + e.name + ": " + String(e.message).split("\n")[0]);
  }
}
for (const k of Object.getOwnPropertyNames(Reflect)) {
  if (k === "apply" || k === "construct") continue;
  t("Reflect." + k + "(1)", () => Reflect[k](1, "a", {}));
}
for (const key of ["global", "ignoreCase", "multiline", "dotAll", "unicode", "unicodeSets", "sticky", "hasIndices", "source"]) {
  const get = Object.getOwnPropertyDescriptor(RegExp.prototype, key).get;
  t("RegExp.prototype." + key + " getter {}", () => get.call({}));
  t("RegExp.prototype." + key + " getter 1", () => get.call(1));
}
for (const C of [Int8Array, Uint8Array, Uint8ClampedArray, Int16Array, Uint16Array, Int32Array, Uint32Array, Float32Array, Float64Array, BigInt64Array, BigUint64Array]) {
  t(C.name + "()", () => C());
}
out.sort();
console.log(out.join("\n"));
