// Execution semantics of the WebAssembly JS API (issue #1378): calls and value coercion, traps, linear
// memory aliasing and detachment, imports (callbacks, exceptions, re-entrancy), tables, globals, start
// functions, multi-value, and the promise API. Stdout must equal Node's.
"use strict";
const { build, op } = require("./builder.js");
const { emit, attempt, attemptAsync, flush } = require("./common.js");

const instantiate = (spec, imports) =>
  new WebAssembly.Instance(new WebAssembly.Module(build(spec)), imports);

async function main() {
  // ---- arithmetic, coercion, traps ----
  const arith = instantiate({
    types: [
      [["i32", "i32"], ["i32"]],
      [["i64", "i64"], ["i64"]],
      [["f32", "f32"], ["f32"]],
      [["f64", "f64"], ["f64"]],
      [[], []],
      [["i32", "i32"], ["i32", "i32"]],
      [["i32"], ["i32"]],
    ],
    funcs: [0, 1, 2, 3, 0, 4, 5, 6],
    exports: [
      ["add", "func", 0], ["add64", "func", 1], ["addf32", "func", 2], ["addf64", "func", 3],
      ["div", "func", 4], ["trap", "func", 5], ["swap", "func", 6], ["recurse", "func", 7],
    ],
    codes: [
      { body: [op.local_get(0), op.local_get(1), op.i32_add] },
      { body: [op.local_get(0), op.local_get(1), op.i64_add] },
      { body: [op.local_get(0), op.local_get(1), op.f32_add] },
      { body: [op.local_get(0), op.local_get(1), op.f64_add] },
      { body: [op.local_get(0), op.local_get(1), op.i32_div_s] },
      { body: [op.unreachable] },
      { body: [op.local_get(1), op.local_get(0)] },
      { body: [op.local_get(0), op.i32_const(1), op.i32_add, op.call(7)] },
    ],
  });
  const x = arith.exports;
  emit("exports.keys", Object.keys(x));
  emit("exports.frozen", [Object.isFrozen(x), Object.getPrototypeOf(x)]);
  emit("add.meta", [x.add.name, x.add.length, x.add64.name, typeof x.add.prototype]);
  attempt("add.new", () => new x.add(1, 2));
  emit("add", [x.add(1, 2), x.add(2 ** 31 - 1, 1), x.add("3", 4), x.add(1.9, 2.9), x.add(undefined, 1), x.add(-1, 4294967297)]);
  emit("add.valueOf", x.add({ valueOf: () => 40 }, 2));
  attempt("add.symbol", () => x.add(Symbol(), 1));
  attempt("add.bigint", () => x.add(1n, 1));
  emit("add64", [x.add64(1n, 2n), x.add64(2n ** 63n - 1n, 1n), x.add64(-1n, 2n ** 64n), x.add64("5", true)]);
  attempt("add64.number", () => x.add64(1, 2));
  attempt("add64.missing", () => x.add64());
  emit("addf32", [x.addf32(0.1, 0.2), x.addf32(1e39, 1), x.addf32(NaN, 1)]);
  emit("addf64", [x.addf64(0.1, 0.2), x.addf64(-0, -0), x.addf64(Infinity, 1)]);
  attempt("div.zero", () => x.div(1, 0), true);
  attempt("div.overflow", () => x.div(-2147483648, -1), true);
  emit("div", x.div(7, -2));
  attempt("trap.unreachable", () => x.trap(), true);
  emit("swap", x.swap(1, 2));
  attempt("recurse", () => x.recurse(0), true);
  attempt("trap.instanceof", () => {
    try {
      x.trap();
    } catch (e) {
      return [e instanceof WebAssembly.RuntimeError, e instanceof Error, Object.getPrototypeOf(WebAssembly.RuntimeError) === Error];
    }
  });
  emit("after-trap", x.add(5, 6));

  // ---- linear memory ----
  const mem = instantiate({
    types: [[["i32", "i32"], []], [["i32"], ["i32"]], [[], ["i32"]]],
    mems: [[1, 2]],
    funcs: [0, 1, 2, 1],
    exports: [["memory", "memory", 0], ["store", "func", 0], ["load", "func", 1], ["size", "func", 2], ["grow", "func", 3]],
    codes: [
      { body: [op.local_get(0), op.local_get(1), op.i32_store()] },
      { body: [op.local_get(0), op.i32_load()] },
      { body: [op.memory_size] },
      { body: [op.local_get(0), op.memory_grow] },
    ],
    datas: [{ offset: 8, bytes: Array.from(new TextEncoder().encode("hello")) }],
  }).exports;
  const b0 = mem.memory.buffer;
  emit("mem.buffer", [b0.byteLength, b0 === mem.memory.buffer, b0 instanceof ArrayBuffer, Object.isFrozen(b0)]);
  emit("mem.data", new Uint8Array(b0, 8, 5));
  mem.store(100, 0x01020304);
  emit("mem.store", new Uint8Array(b0, 100, 4));
  new Uint8Array(b0)[200] = 9;
  emit("mem.load", [mem.load(200), mem.load(100)]);
  const view = new Uint8Array(b0);
  const dv = new DataView(b0);
  dv.setInt32(300, -2, true);
  emit("mem.dataview", mem.load(300));
  emit("mem.size", mem.size());
  attempt("mem.oob", () => mem.load(65536), true);
  attempt("mem.oob.store", () => mem.store(65534, 1), true);
  emit("mem.grow", mem.grow(1));
  emit("mem.detached", [b0.byteLength, view.length, b0.detached, mem.memory.buffer.byteLength, mem.memory.buffer === b0]);
  attempt("mem.old-view", () => view[0]);
  attempt("mem.old-dataview", () => dv.getInt32(0));
  const b1 = mem.memory.buffer;
  emit("mem.after", [mem.load(100), new Uint8Array(b1, 100, 4), mem.size()]);
  emit("mem.grow-fail", mem.grow(1));
  emit("mem.js-grow", [(() => { try { return mem.memory.grow(1); } catch (e) { return e.name; } })()]);
  emit("mem.js-grow0", [mem.memory.grow(0), b1.byteLength, mem.memory.buffer === b1]);
  attempt("mem.transfer", () => mem.memory.buffer.transfer());
  attempt("mem.structured", () => structuredClone(mem.memory.buffer, { transfer: [mem.memory.buffer] }));
  emit("mem.clone", structuredClone(mem.memory.buffer).byteLength);
  const own = new WebAssembly.Memory({ initial: 1, maximum: 3 });
  const ob = own.buffer;
  emit("Memory.new", [ob.byteLength, own.grow(1), ob.byteLength, own.buffer.byteLength]);
  attempt("Memory.grow-max", () => own.grow(2), true);
  attempt("Memory.no-desc", () => new WebAssembly.Memory(), true);
  attempt("Memory.no-initial", () => new WebAssembly.Memory({}), true);
  attempt("Memory.big", () => new WebAssembly.Memory({ initial: 70000 }), true);
  attempt("Memory.max<initial", () => new WebAssembly.Memory({ initial: 2, maximum: 1 }), true);
  attempt("Memory.negative", () => new WebAssembly.Memory({ initial: -1 }), true);
  attempt("Memory.nan", () => new WebAssembly.Memory({ initial: "x" }), true);
  attempt("Memory.receiver", () => WebAssembly.Memory.prototype.grow.call({}, 1), true);
  attempt("Memory.call", () => WebAssembly.Memory({ initial: 1 }));
  emit("Memory.tag", [String(own), Object.prototype.toString.call(own.buffer)]);
  const shared = instantiate({
    mems: [[1]], exports: [["memory", "memory", 0]],
  }).exports.memory;
  emit("Memory.shared", [shared instanceof WebAssembly.Memory, shared.buffer.byteLength]);

  // ---- imports ----
  const importSpec = {
    types: [
      [["i32"], ["i32"]],
      [["i32", "f64", "i64"], ["f64"]],
      [[], []],
      [[], ["i32"]],
      [["i32"], ["f64"]],
    ],
    imports: [
      { module: "env", name: "add1", kind: "func", type: 0 },
      { module: "env", name: "sum", kind: "func", type: 1 },
      { module: "env", name: "boom", kind: "func", type: 2 },
      { module: "env", name: "g", kind: "global", type: "i32", mutable: false },
    ],
    funcs: [0, 4, 2, 3],
    exports: [["go", "func", 3], ["callsum", "func", 4], ["callboom", "func", 5], ["readg", "func", 6]],
    codes: [
      { body: [op.local_get(0), op.call(0), op.call(0)] },
      { body: [op.local_get(0), op.f64_const(2.5), op.i64_const(-3), op.call(1)] },
      { body: [op.call(2)] },
      { body: [op.global_get(0)] },
    ],
  };
  const calls = [];
  const thrown = new Error("from js");
  let mode = "ok";
  const imp = instantiate(importSpec, {
    env: {
      add1(v) { calls.push(["add1", typeof v, v]); return mode === "str" ? "5" : v + 1; },
      sum(a, b, c) { calls.push(["sum", a, b, c]); return mode === "obj" ? { valueOf: () => 7 } : a + b + Number(c); },
      boom() { throw thrown; },
      g: 42,
    },
  }).exports;
  emit("imp.go", imp.go(1));
  emit("imp.calls", calls);
  emit("imp.callsum", imp.callsum(10));
  mode = "str";
  emit("imp.coerce.str", imp.go(1));
  mode = "obj";
  emit("imp.coerce.obj", imp.callsum(1));
  attempt("imp.throw", () => { try { imp.callboom(); } catch (e) { return e === thrown; } });
  emit("imp.after-throw", imp.readg());
  attempt("imp.missing-module", () => instantiate(importSpec, {}), true);
  attempt("imp.no-object", () => instantiate(importSpec), true);
  attempt("imp.missing-func", () => instantiate(importSpec, { env: { g: 1 } }), true);
  attempt("imp.global-type", () => instantiate(importSpec, { env: { add1() {}, sum() {}, boom() {}, g: 1n } }));
  attempt("imp.global-obj", () => instantiate(importSpec, { env: { add1() {}, sum() {}, boom() {}, g: new WebAssembly.Global({ value: "i32" }, 7) } }).exports.readg());
  attempt("imp.global-mut", () => instantiate(importSpec, { env: { add1() {}, sum() {}, boom() {}, g: new WebAssembly.Global({ value: "i32", mutable: true }, 7) } }));
  let reentered = 0;
  const re = instantiate(importSpec, {
    env: {
      add1(v) { return reentered++ < 3 ? re.exports.go(v) + 1 : v; },
      sum() { return 0; }, boom() {}, g: 0,
    },
  });
  emit("imp.reentrant", [re.exports.go(0), reentered]);
  attempt("imp.wasm-func-mismatch", () => instantiate(importSpec, { env: { add1: x.div, sum() { return 0; }, boom() {}, g: 0 } }));
  const sameType = instantiate(importSpec, { env: { add1: x.recurse, sum() { return 0; }, boom() {}, g: 0 } });
  attempt("imp.wasm-func-direct", () => sameType.exports.go(1));
  const dep = instantiate({
    types: [[["i32"], ["i32"]]], imports: [{ module: "m", name: "f", kind: "func", type: 0 }],
    funcs: [0], exports: [["f", "func", 0], ["g", "func", 1]], codes: [{ body: [op.local_get(0), op.call(0)] }],
  }, { m: { f: instantiate({ types: [[["i32"], ["i32"]]], funcs: [0], exports: [["inc", "func", 0]], codes: [{ body: [op.local_get(0), op.i32_const(1), op.i32_add] }] }).exports.inc } });
  emit("imp.reexport", [dep.exports.g(4), dep.exports.f(4), dep.exports.f.name]);

  // ---- tables ----
  const tbl = instantiate({
    types: [[["i32"], ["i32"]], [["i32", "i32"], ["i32"]]],
    funcs: [0, 0, 1],
    tables: [["funcref", 4, 8]],
    exports: [["inc", "func", 0], ["dbl", "func", 1], ["call", "func", 2], ["tbl", "table", 0]],
    elems: [{ offset: 0, funcs: [0, 1] }],
    codes: [
      { body: [op.local_get(0), op.i32_const(1), op.i32_add] },
      { body: [op.local_get(0), op.i32_const(2), op.i32_mul] },
      { body: [op.local_get(1), op.local_get(0), op.call_indirect(0)] },
    ],
  }).exports;
  emit("tbl.call", [tbl.call(0, 5), tbl.call(1, 5)]);
  attempt("tbl.call-null", () => tbl.call(2, 5), true);
  attempt("tbl.call-oob", () => tbl.call(10, 5), true);
  emit("tbl.get", [tbl.tbl.get(0) === tbl.inc, tbl.tbl.get(1) === tbl.dbl, tbl.tbl.get(2), tbl.tbl.length]);
  emit("tbl.grow", [tbl.tbl.grow(2), tbl.tbl.length]);
  tbl.tbl.set(2, tbl.dbl);
  emit("tbl.set", [tbl.call(2, 7), tbl.tbl.get(2) === tbl.dbl]);
  tbl.tbl.set(2, null);
  emit("tbl.set-null", tbl.tbl.get(2));
  attempt("tbl.set-js", () => tbl.tbl.set(3, () => 1), true);
  attempt("tbl.get-oob", () => tbl.tbl.get(100), true);
  attempt("tbl.grow-max", () => tbl.tbl.grow(10), true);
  attempt("tbl.receiver", () => Object.getOwnPropertyDescriptor(WebAssembly.Table.prototype, "length").get.call({}), true);
  const ext = new WebAssembly.Table({ element: "externref", initial: 2 }, "x");
  const obj = { a: 1 };
  ext.set(1, obj);
  emit("ext", [ext.get(0), ext.get(1) === obj, ext.grow(1, 5), ext.get(2), ext.length]);
  const fresh = new WebAssembly.Table({ element: "anyfunc", initial: 1 });
  fresh.set(0, tbl.inc);
  emit("tbl.new", [fresh.get(0) === tbl.inc, fresh.length]);
  attempt("Table.bad-element", () => new WebAssembly.Table({ element: "i32", initial: 1 }), true);
  attempt("Table.no-initial", () => new WebAssembly.Table({ element: "anyfunc" }), true);

  // ---- globals and the start function ----
  const glob = instantiate({
    types: [[[], []], [[], ["i32"]]],
    globals: [
      { type: "i32", mutable: true, init: op.i32_const(5) },
      { type: "i64", mutable: false, init: op.i64_const(7) },
      { type: "f64", mutable: true, init: op.f64_const(1.5) },
    ],
    funcs: [0, 1],
    exports: [["g0", "global", 0], ["g1", "global", 1], ["g2", "global", 2], ["inc", "func", 0], ["get", "func", 1]],
    start: 0,
    codes: [
      { body: [op.global_get(0), op.i32_const(1), op.i32_add, op.global_set(0)] },
      { body: [op.global_get(0)] },
    ],
  }).exports;
  emit("glob.start", [glob.g0.value, glob.get()]);
  glob.inc();
  emit("glob.inc", [glob.g0.value, glob.g0.valueOf(), glob.g1.value, glob.g2.value]);
  glob.g0.value = 9.7;
  glob.g2.value = "2.25";
  emit("glob.set", [glob.g0.value, glob.get(), glob.g2.value]);
  attempt("glob.immutable", () => { glob.g1.value = 1n; });
  attempt("glob.bad-set", () => { glob.g0.value = Symbol(); });
  const g64 = new WebAssembly.Global({ value: "i64", mutable: true }, 5n);
  g64.value = 2n ** 64n + 3n;
  emit("Global.new", [new WebAssembly.Global({ value: "i32" }).value, new WebAssembly.Global({ value: "i64" }).value, g64.value, new WebAssembly.Global({ value: "f32" }, 1.1).value, new WebAssembly.Global({ value: "externref" }).value, new WebAssembly.Global({ value: "anyfunc" }).value, String(g64)]);
  attempt("Global.bad-value", () => new WebAssembly.Global({ value: "x" }), true);
  attempt("Global.no-desc", () => new WebAssembly.Global(), true);
  attempt("Global.i64-number", () => new WebAssembly.Global({ value: "i64" }, 5));

  // ---- promises ----
  const bytes = build({
    types: [[["i32"], ["i32"]]], funcs: [0], exports: [["inc", "func", 0]],
    codes: [{ body: [op.local_get(0), op.i32_const(1), op.i32_add] }],
  });
  const compiled = await WebAssembly.compile(bytes);
  emit("compile", [compiled instanceof WebAssembly.Module, WebAssembly.Module.exports(compiled)]);
  const both = await WebAssembly.instantiate(bytes);
  emit("instantiate.bytes", [Object.keys(both), both.module instanceof WebAssembly.Module, both.instance instanceof WebAssembly.Instance, both.instance.exports.inc(1)]);
  const inst = await WebAssembly.instantiate(compiled);
  emit("instantiate.module", [inst instanceof WebAssembly.Instance, inst.exports.inc(2)]);
  await attemptAsync("compile.bad", () => WebAssembly.compile(new Uint8Array(8)));
  await attemptAsync("compile.type", () => WebAssembly.compile(1), true);
  await attemptAsync("instantiate.type", () => WebAssembly.instantiate({}));
  await attemptAsync("instantiate.imports-type", () => WebAssembly.instantiate(bytes, 5), true);
  await attemptAsync("instantiate.link", () => WebAssembly.instantiate(build(importSpec), { env: {} }));
  emit("compile.promise", [WebAssembly.compile(bytes) instanceof Promise, WebAssembly.instantiate(bytes) instanceof Promise]);

  flush();
}

main().catch((e) => {
  console.log("FAILED", e && e.stack);
  process.exitCode = 1;
});
