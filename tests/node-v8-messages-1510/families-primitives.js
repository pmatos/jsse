// V8 message text of builtin errors by family (#1510): primitive wrapper receivers.
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
const brands = { Number, Boolean, Symbol, BigInt };
for (const [name, C] of Object.entries(brands)) {
  for (const key of Object.getOwnPropertyNames(C.prototype)) {
    const d = Object.getOwnPropertyDescriptor(C.prototype, key);
    if (key === "constructor") continue;
    const fns = [];
    if (typeof d.value === "function") fns.push([key, d.value]);
    if (d.get) fns.push(["get " + key, d.get]);
    for (const [label, fn] of fns) {
      t(name + ".prototype." + label + ".call(1)", () => fn.call(1, 1, 1));
      t(name + ".prototype." + label + ".call({})", () => fn.call({}, 1, 1));
      t(name + ".prototype." + label + ".call([])", () => fn.call([], 1, 1));
    }
  }
}
out.sort();
console.log(out.join("\n"));
