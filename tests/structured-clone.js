// HTML host global structuredClone (jsse#1188). Host behaviour rather than
// ECMAScript, so it lives here and not in test262-extra.

function assert(cond, msg) {
  if (!cond) throw new Error(msg);
}

function dataCloneError(fn, msg) {
  try {
    fn();
  } catch (e) {
    assert(e instanceof Error, msg + ": must be an Error");
    assert(e.name === "DataCloneError" && e.code === 25, msg + ": got " + e.name + "/" + e.code);
    return;
  }
  throw new Error(msg + ": did not throw");
}

function throwsType(fn, msg) {
  try {
    fn();
  } catch (e) {
    assert(e instanceof TypeError, msg + ": must be a TypeError, got " + e);
    return;
  }
  throw new Error(msg + ": did not throw");
}

assert(typeof structuredClone === "function" && structuredClone.length === 1, "shape");
throwsType(() => structuredClone(), "missing argument");

// Primitives pass through unchanged.
assert(structuredClone(1) === 1 && structuredClone("s") === "s", "number/string");
assert(structuredClone(null) === null && structuredClone(undefined) === undefined, "nullish");
assert(Object.is(structuredClone(-0), -0), "-0 survives");
assert(structuredClone(10n) === 10n, "bigint");
dataCloneError(() => structuredClone(Symbol("x")), "symbol");
dataCloneError(() => structuredClone({ a: Symbol() }), "nested symbol");
dataCloneError(() => structuredClone(Object(Symbol())), "Symbol wrapper");

// Plain objects: prototype dropped, accessors flattened, non-enumerable and
// Symbol keys omitted.
class Point {
  constructor() {
    this.x = 1;
  }
}
const src = new Point();
Object.defineProperty(src, "hidden", { value: 1, enumerable: false });
Object.defineProperty(src, "g", { get: () => 7, enumerable: true });
src[Symbol("s")] = 1;
const plain = structuredClone(src);
assert(Object.getPrototypeOf(plain) === Object.prototype, "class prototype is dropped");
assert(Object.getOwnPropertyNames(plain).join() === "x,g", "own enumerable string keys");
assert(Object.getOwnPropertyDescriptor(plain, "g").value === 7, "accessor becomes data");
assert(Object.getOwnPropertySymbols(plain).length === 0, "symbol keys dropped");
assert(structuredClone(Object.create(null)) !== null, "null-prototype object");

// Arrays keep holes and extra named properties.
const holes = [1, , 3];
holes.extra = "e";
const arr = structuredClone(holes);
assert(Array.isArray(arr) && arr.length === 3, "array");
assert(!(1 in arr) && arr[2] === 3 && arr.extra === "e", "holes and extra key");

// Cycles and shared references.
const cyc = { list: [] };
cyc.self = cyc;
cyc.list.push(cyc);
const cycClone = structuredClone(cyc);
assert(cycClone !== cyc && cycClone.self === cycClone && cycClone.list[0] === cycClone, "cycle");
const shared = { n: 1 };
const sharedClone = structuredClone({ a: shared, b: shared });
assert(sharedClone.a === sharedClone.b && sharedClone.a !== shared, "shared reference");

// Wrappers, Date, RegExp.
assert(typeof structuredClone(Object("s")) === "object", "String wrapper");
assert(structuredClone(Object(1n)).valueOf() === 1n, "BigInt wrapper");
assert(structuredClone(Object(true)).valueOf() === true, "Boolean wrapper");
assert(structuredClone(Object(5)).valueOf() === 5, "Number wrapper");
const date = new Date(86400000);
date.tag = "dropped";
const dateClone = structuredClone(date);
assert(dateClone instanceof Date && dateClone !== date, "Date identity");
assert(dateClone.getTime() === 86400000 && dateClone.tag === undefined, "Date value only");
assert(Number.isNaN(structuredClone(new Date(NaN)).getTime()), "invalid Date");
const re = /a(b)/gi;
re.lastIndex = 3;
const reClone = structuredClone(re);
assert(reClone instanceof RegExp && reClone.source === "a(b)" && reClone.flags === "gi", "RegExp");
assert(reClone.lastIndex === 0, "lastIndex resets");

// Map and Set, with cloned keys and values.
const key = { k: 1 };
const map = structuredClone(new Map([[key, new Set([1, { s: 2 }])]]));
assert(map instanceof Map && map.size === 1, "Map");
const [mk, mv] = [...map][0];
assert(mk !== key && mk.k === 1 && mv instanceof Set && mv.size === 2, "Map entry");

// Errors keep a native name, message, cause; lose extras and subclasses.
const err = new RangeError("boom", { cause: { why: 1 } });
err.extra = 1;
const errClone = structuredClone(err);
assert(errClone instanceof RangeError && errClone.name === "RangeError", "RangeError");
assert(errClone.message === "boom" && errClone.cause.why === 1, "message and cause");
assert(!("extra" in errClone), "extra properties dropped");
class AppError extends Error {}
assert(Object.getPrototypeOf(structuredClone(new AppError("a"))) === Error.prototype, "subclass");
const renamed = new Error("x");
renamed.name = "Custom";
assert(structuredClone(renamed).name === "Error", "unknown name falls back to Error");

// ArrayBuffers and views.
const buf = new ArrayBuffer(8);
const u8 = new Uint8Array(buf, 2, 3);
const u16 = new Uint16Array(buf, 4);
u8[0] = 9;
const [c8, c16] = structuredClone([u8, u16]);
assert(c8.buffer !== buf && c8.buffer === c16.buffer, "views share one cloned buffer");
assert(c8.byteOffset === 2 && c8.length === 3 && c8[0] === 9, "Uint8Array view");
assert(c16.byteOffset === 4 && c16.length === 2, "Uint16Array view");
u8[0] = 1;
assert(c8[0] === 9, "clone does not alias");
const view = structuredClone(new DataView(new ArrayBuffer(4), 1, 2));
assert(view.byteOffset === 1 && view.byteLength === 2, "DataView");
const growable = structuredClone(new ArrayBuffer(4, { maxByteLength: 16 }));
assert(growable.resizable && growable.maxByteLength === 16, "resizable buffer");
const sab = new SharedArrayBuffer(4);
assert(structuredClone(sab).byteLength === 4, "SharedArrayBuffer");

// transfer detaches the source and checks the list.
const moved = new ArrayBuffer(8);
const view8 = new Uint8Array(moved);
const target = structuredClone({ view8 }, { transfer: [moved] });
assert(moved.byteLength === 0 && view8.length === 0, "source detached");
assert(target.view8.buffer.byteLength === 8, "transferred buffer");
dataCloneError(() => structuredClone(moved), "detached buffer");
const twice = new ArrayBuffer(1);
dataCloneError(() => structuredClone(1, { transfer: [twice, twice] }), "duplicate transfer");
dataCloneError(() => structuredClone(1, { transfer: [{}] }), "non-buffer transfer");
assert(twice.byteLength === 1, "a rejected transfer leaves the source intact");
const failing = new ArrayBuffer(2);
dataCloneError(() => structuredClone(() => {}, { transfer: [failing] }), "failure after transfer");
assert(failing.byteLength === 2, "a failed clone does not detach");

// Options validation.
assert(structuredClone(1, null) === 1 && structuredClone(1, undefined) === 1, "nullish options");
assert(structuredClone(1, { transfer: undefined }) === 1, "undefined transfer");
throwsType(() => structuredClone(1, 2), "primitive options");
throwsType(() => structuredClone(1, { transfer: 5 }), "non-iterable transfer");
throwsType(() => structuredClone(1, { transfer: [1] }), "primitive transfer entry");

// Uncloneable objects.
dataCloneError(() => structuredClone(() => {}), "function");
dataCloneError(() => structuredClone({ f() {} }), "method");
dataCloneError(() => structuredClone(new Proxy({}, {})), "proxy");
dataCloneError(() => structuredClone(new WeakMap()), "WeakMap");
dataCloneError(() => structuredClone(new WeakSet()), "WeakSet");
dataCloneError(() => structuredClone(new WeakRef({})), "WeakRef");
dataCloneError(() => structuredClone(Promise.resolve()), "Promise");
dataCloneError(() => structuredClone([].values()), "iterator");
dataCloneError(() => structuredClone((function* () {})()), "generator");

// Getters run in key order, and a collection triggered by one cannot free the
// partial clone.
const order = [];
const probe = {
  get a() {
    order.push("a");
    if (typeof $262 !== "undefined") $262.gc();
    return { deep: [1, 2, { three: 3 }] };
  },
  get b() {
    order.push("b");
    if (typeof $262 !== "undefined") $262.gc();
    return new Map([[1, { v: "m" }]]);
  },
  c: [{ z: 1 }],
};
const probed = structuredClone(probe);
assert(order.join() === "a,b", "getter order");
assert(probed.a.deep[2].three === 3 && probed.b.get(1).v === "m" && probed.c[0].z === 1, "values");

// A getter that deletes a not-yet-visited property: the key is skipped.
const mutating = {
  get first() {
    delete this.second;
    return 1;
  },
  second: 2,
};
assert(!("second" in structuredClone(mutating)), "deleted key is skipped");

// Deep nesting fails cleanly instead of overflowing the native stack.
let deep = {};
for (let i = 0; i < 100000; i++) deep = { deep };
try {
  structuredClone(deep);
  throw new Error("deep nesting should throw");
} catch (e) {
  assert(e instanceof RangeError, "deep nesting must be a RangeError, got " + e);
}
