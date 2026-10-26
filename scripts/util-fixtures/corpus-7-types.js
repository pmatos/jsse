// util.types over every object kind
var typeNames = Object.keys(types).filter(function (k) { return typeof types[k] === "function"; }).sort();
emit("types-names => " + typeNames.join(","));
function mark(label, value) {
  var hits = typeNames.filter(function (name) {
    try { return types[name](value) === true; } catch (e) { return false; }
  });
  emit("types " + label + " => " + hits.join(","));
}
class TA extends Uint8Array {}
class MapSub extends Map {}
function* gen() {}
async function* agen() {}
var ab = new ArrayBuffer(8);
var values = {
  undefined: undefined, null: null, number: 1, string: "s", bool: true, symbol: Symbol("x"), bigint: 1n,
  obj: {}, arr: [], fn: function () {}, arrow: () => {}, asyncFn: async function () {}, asyncArrow: async () => {},
  genFn: gen, asyncGenFn: agen, cls: class {}, bound: function () {}.bind(null), method: ({ m() {} }).m,
  date: new Date(0), regexp: /x/, error: new Error("e"), typeError: new TypeError("t"), rangeError: new RangeError("r"),
  aggregate: new AggregateError([]), evalError: new EvalError("e"), uriError: new URIError("u"), syntaxError: new SyntaxError("s"),
  refError: new ReferenceError("r"), customError: new (class CE extends Error {})("c"), errorLike: { name: "Error", message: "m" },
  errorProto: Object.create(Error.prototype), errorProtoItself: Error.prototype,
  map: new Map(), set: new Set(), weakMap: new WeakMap(), weakSet: new WeakSet(), mapSub: new MapSub(),
  mapProto: Map.prototype, setProto: Set.prototype, weakMapProto: WeakMap.prototype,
  mapIter: new Map().entries(), setIter: new Set().values(), arrIter: [].values(), strIter: ""[Symbol.iterator](),
  promise: Promise.resolve(), promiseProto: Promise.prototype, genObj: gen(), asyncGenObj: agen(),
  ab: ab, sab: new SharedArrayBuffer(8), dv: new DataView(ab), abProto: ArrayBuffer.prototype,
  int8: new Int8Array(1), uint8: new Uint8Array(1), uint8c: new Uint8ClampedArray(1), int16: new Int16Array(1), uint16: new Uint16Array(1),
  int32: new Int32Array(1), uint32: new Uint32Array(1), float32: new Float32Array(1), float64: new Float64Array(1),
  bigInt64: new BigInt64Array(1), bigUint64: new BigUint64Array(1), taSub: new TA(1), taProto: Uint8Array.prototype,
  numberObj: new Number(1), stringObj: new String("s"), booleanObj: new Boolean(true), symbolObj: Object(Symbol("o")), bigintObj: Object(1n),
  numberProto: Number.prototype, stringProto: String.prototype, booleanProto: Boolean.prototype,
  dateProto: Date.prototype, regexpProto: RegExp.prototype,
  proxy: new Proxy({}, {}), proxyFn: new Proxy(function () {}, {}), proxyArr: new Proxy([], {}), proxyMap: new Proxy(new Map(), {}),
  proxyDate: new Proxy(new Date(), {}), proxyPromise: new Proxy(Promise.resolve(), {}),
  args: (function () { return arguments; })(), strictArgs: (function () { "use strict"; return arguments; })(),
  weakref: typeof WeakRef === "function" ? new WeakRef({}) : {}, globalThis: globalThis, math: Math, json: JSON, reflect: Reflect,
  buffer: Buffer.from("a"), float16: new Float16Array(1), finreg: new FinalizationRegistry(function () {}), weakref2: new WeakRef({}),
  disposableStack: new DisposableStack(), asyncDisposableStack: new AsyncDisposableStack(), suppressed: new SuppressedError(1, 2, "s"),
  iterHelper: [].values().map(function (x) { return x; }), iterProto2: Iterator.prototype, domException: new DOMException("d", "AbortError"),
  intlNumber: new Intl.NumberFormat("en"), intlDate: new Intl.DateTimeFormat("en"), intlCollator: new Intl.Collator("en"),
  segmenter: new Intl.Segmenter("en"), segments: new Intl.Segmenter("en").segment("ab"), segmentIter: new Intl.Segmenter("en").segment("ab")[Symbol.iterator](),
  temporalDate: Temporal.PlainDate.from("2020-01-02"), temporalInstant: Temporal.Instant.fromEpochMilliseconds(0), temporalDuration: Temporal.Duration.from("PT1S"),
  regexpIter: "a".matchAll(/a/g), eventTarget: new EventTarget(), abortController: new AbortController(), url: typeof URL === "function" ? new URL("http://a/") : {},
  textDecoder: new TextDecoder(), textEncoder: new TextEncoder(), structured: structuredClone(new Map([[1, 2]])), timerObj: setTimeout(function () {}, 0), iterProto: Object.getPrototypeOf(Object.getPrototypeOf([].values())),
};
Object.keys(values).forEach(function (k) { mark(k, values[k]); });
var nsLike = typeof types.isModuleNamespaceObject === "function";
emit("types-ns => " + nsLike + " " + types.isModuleNamespaceObject({}));
emit("types-external => " + types.isExternal({}) + " " + types.isExternal(1));
emit("types-keyobject => " + types.isKeyObject({}) + " " + types.isCryptoKey({}));
emit("types-arraybufferview => " + [new Uint8Array(1), new DataView(ab), ab, {}, null].map(function (v) { return types.isArrayBufferView(v); }).join(","));
emit("types-anyab => " + [ab, new SharedArrayBuffer(1), new Uint8Array(1)].map(function (v) { return types.isAnyArrayBuffer(v); }).join(","));
emit("types-boxed => " + [new Number(1), 1, Object(1n), Object(Symbol()), new String("")].map(function (v) { return types.isBoxedPrimitive(v); }).join(","));
emit("types-typedarray => " + [new Uint8Array(1), new DataView(ab), ab, [], new TA(1)].map(function (v) { return types.isTypedArray(v); }).join(","));
emit("types-proto-tag => " + Object.prototype.toString.call(types));
clearTimeout(values.timerObj);
