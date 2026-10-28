// The shape of the WebAssembly JS API (issue #1378): property attributes, constructors, brand
// checks, Module.imports/exports/customSections, validate, and error classes. Stdout must equal Node's.
"use strict";
const { build, op } = require("./builder.js");
const { emit, attempt, flush } = require("./common.js");

const W = WebAssembly;
const desc = (o, k) => {
  const d = Object.getOwnPropertyDescriptor(o, k);
  return d === undefined ? "absent" : ["w" + d.writable, "e" + d.enumerable, "c" + d.configurable, d.get ? "get" : "", d.set ? "set" : ""].join("");
};

emit("global", [typeof W, desc(globalThis, "WebAssembly"), Object.prototype.toString.call(W), Object.getPrototypeOf(W) === Object.prototype]);
for (const name of ["compile", "validate", "instantiate", "Module", "Instance", "Table", "Memory", "Global", "CompileError", "LinkError", "RuntimeError"]) {
  emit("ns." + name, [typeof W[name], W[name].name, W[name].length, desc(W, name)]);
}
for (const name of ["Module", "Instance", "Table", "Memory", "Global"]) {
  const proto = W[name].prototype;
  emit("proto." + name, [Object.getOwnPropertyNames(proto).filter((k) => k !== "constructor" && !/^to(Fixed|Resiz)/.test(k)).sort(), Object.prototype.toString.call(Object.create(proto)), desc(proto, Symbol.toStringTag)]);
}
emit("proto.members", [desc(W.Memory.prototype, "buffer"), desc(W.Memory.prototype, "grow"), desc(W.Table.prototype, "length"), desc(W.Global.prototype, "value"), desc(W.Instance.prototype, "exports")]);
emit("Module.statics", [Object.getOwnPropertyNames(W.Module).filter((k) => !["length", "name", "prototype"].includes(k)).sort(), W.Module.imports.length, W.Module.exports.length, W.Module.customSections.length]);
for (const name of ["CompileError", "LinkError", "RuntimeError"]) {
  const E = W[name];
  const e = new E("boom");
  emit("error." + name, [e instanceof E, e instanceof Error, e.name, e.message, String(e), Object.getPrototypeOf(E) === Error, Object.getPrototypeOf(E.prototype) === Error.prototype, E(("x")) instanceof E, new E().message === "", desc(E.prototype, "name"), desc(E.prototype, "message"), Object.keys(e)]);
}
attempt("error.cause", () => new W.RuntimeError("m", { cause: 1 }).cause);

const bytes = build({
  types: [[["i32"], ["i32"]]],
  imports: [
    { module: "env", name: "f", kind: "func", type: 0 },
    { module: "env", name: "m", kind: "memory", limits: [1, 2] },
    { module: "env", name: "t", kind: "table", limits: [1] },
    { module: "env", name: "g", kind: "global", type: "i32", mutable: false },
    { module: "other", name: "f2", kind: "func", type: 0 },
  ],
  funcs: [0],
  exports: [["run", "func", 2], ["mem", "memory", 0], ["z", "memory", 0], ["a", "func", 0]],
  codes: [{ body: [op.local_get(0)] }],
  customs: [["note", [1, 2, 3]], ["other", [9]], ["note", []]],
});
const mod = new W.Module(bytes);
emit("Module.imports", W.Module.imports(mod));
emit("Module.exports", W.Module.exports(mod));
emit("Module.custom", [W.Module.customSections(mod, "note").map((b) => [b instanceof ArrayBuffer, Array.from(new Uint8Array(b))]), W.Module.customSections(mod, "none"), W.Module.customSections(mod, "other").length]);
attempt("Module.custom.noname", () => W.Module.customSections(mod), true);
attempt("Module.imports.type", () => W.Module.imports({}), true);
attempt("Module.exports.type", () => W.Module.exports(), true);
attempt("Module.no-new", () => W.Module(bytes), false);
attempt("Module.no-args", () => new W.Module(), true);
attempt("Module.string", () => new W.Module("abc"), true);
attempt("Module.dataview", () => new W.Module(new DataView(bytes.buffer)));
attempt("Module.empty", () => new W.Module(new Uint8Array(0)));
attempt("Module.magic", () => new W.Module(new Uint8Array([0, 97, 115, 109, 2, 0, 0, 0])));
attempt("Module.truncated", () => new W.Module(bytes.slice(0, bytes.length - 3)));
const views = [bytes, bytes.buffer, Buffer.from(bytes), new Uint8Array(bytes.length + 8).fill(0).map((_, i) => (i < bytes.length ? bytes[i] : 0)).subarray(0, bytes.length)];
emit("Module.sources", views.map((v) => new W.Module(v) instanceof W.Module));
const offset = new Uint8Array(bytes.length + 5);
offset.set(bytes, 5);
emit("Module.offset-view", new W.Module(offset.subarray(5)) instanceof W.Module);
emit("validate", [W.validate(bytes), W.validate(new Uint8Array(8)), W.validate(new Uint8Array([0, 97, 115, 109, 1, 0, 0, 0])), W.validate(bytes.buffer)]);
attempt("validate.type", () => W.validate("abc"), true);
attempt("validate.none", () => W.validate(), true);

const empty = new W.Instance(new W.Module(new Uint8Array([0, 97, 115, 109, 1, 0, 0, 0])));
emit("Instance", [Object.keys(empty.exports), Object.isFrozen(empty.exports), Object.getPrototypeOf(empty.exports), empty.exports === empty.exports]);
attempt("Instance.no-module", () => new W.Instance({}), true);
attempt("Instance.bad-imports", () => new W.Instance(mod, 5), true);
attempt("Instance.no-imports", () => new W.Instance(mod), true);
attempt("Instance.call", () => W.Instance(mod));
attempt("Instance.receiver", () => Object.getOwnPropertyDescriptor(W.Instance.prototype, "exports").get.call({}), true);
attempt("Instance.exports-assign", () => {
  "use strict";
  empty.exports.x = 1;
});
const env = { f: (x) => x, m: new W.Memory({ initial: 1, maximum: 2 }), t: new W.Table({ element: "anyfunc", initial: 1 }), g: 3 };
const inst = new W.Instance(mod, { env, other: { f2: (x) => x } });
emit("Instance.exports", [Object.keys(inst.exports), inst.exports.mem === env.m, inst.exports.z === env.m, inst.exports.run(5), inst.exports.run.name, inst.exports.a.name, desc(inst.exports, "run")]);
attempt("Instance.memory-mismatch", () => new W.Instance(mod, { env: { ...env, m: new W.Memory({ initial: 1 }) }, other: { f2() {} } }));
attempt("Instance.memory-small", () => new W.Instance(mod, { env: { ...env, m: new W.Memory({ initial: 0, maximum: 2 }) }, other: { f2() {} } }));
attempt("Instance.memory-type", () => new W.Instance(mod, { env: { ...env, m: {} }, other: { f2() {} } }));
attempt("Instance.table-type", () => new W.Instance(mod, { env: { ...env, t: {} }, other: { f2() {} } }));
attempt("Instance.func-type", () => new W.Instance(mod, { env: { ...env, f: 1 }, other: { f2() {} } }));
attempt("Instance.ns-type", () => new W.Instance(mod, { env: 1, other: { f2() {} } }));
attempt("Instance.getter-order", () => {
  const log = [];
  const proxy = new Proxy({}, { get: (_, k) => { log.push(String(k)); return env; } });
  try { new W.Instance(mod, proxy); } catch (e) { log.push(e.name); }
  return log;
});
flush();
