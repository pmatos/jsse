// util.* functions other than inspect/format/types
emit("exports => " + Object.keys(util).sort().join(","));
// inherits / _extend / isArray / toUSVString / stripVTControlCharacters
function Parent() {}
Parent.prototype.hello = function () { return "hi"; };
function Child() {}
util.inherits(Child, Parent);
emit("inherits => " + (new Child() instanceof Parent) + " " + Child.super_.name + " " + new Child().hello() + " " + Object.getPrototypeOf(Child.prototype).constructor.name);
attempt("inherits-null", function () { util.inherits(null, Parent); });
attempt("inherits-noproto", function () { util.inherits(Child, {}); });
attempt("inherits-super-null", function () { util.inherits(Child, null); });
attempt("extend", function () { return util._extend({ a: 1 }, { b: 2 }); });
attempt("extend-null", function () { return util._extend({ a: 1 }, null); });
attempt("isArray", function () { return [util.isArray([]), util.isArray({})].join(); });
attempt("toUSVString", function () { return JSON.stringify([util.toUSVString("a\ud800b"), util.toUSVString(5), util.toUSVString("ok😀")]); });
attempt("stripVT", function () { return JSON.stringify([util.stripVTControlCharacters("\u001b[31mred\u001b[39m"), util.stripVTControlCharacters("plain"), util.stripVTControlCharacters("\u001b]8;;http://x\u0007link\u001b]8;;\u0007")]); });
attempt("stripVT-bad", function () { return util.stripVTControlCharacters(5); });
// getSystemError*
attempt("sysname", function () { return [util.getSystemErrorName(-2), util.getSystemErrorName(-13), util.getSystemErrorName(-98), util.getSystemErrorName(-12345)].join(); });
attempt("sysname-bad", function () { return util.getSystemErrorName(2); });
attempt("sysname-bad2", function () { return util.getSystemErrorName("x"); });
attempt("sysmap", function () { var m = util.getSystemErrorMap(); return [m instanceof Map, m.size > 50, JSON.stringify(m.get(-2)), JSON.stringify(m.get(-111))].join(); });
attempt("sysmsg", function () { return typeof util.getSystemErrorMessage === "function" ? util.getSystemErrorMessage(-2) : "n/a"; });
attempt("errno-exception", function () { var e = util._errnoException(-2, "open", "extra"); return [e.message, e.errno, e.code, e.syscall].join("|"); });
attempt("host-port", function () { var e = util._exceptionWithHostPort(-111, "connect", "127.0.0.1", 80, "local"); return [e.message, e.errno, e.code, e.syscall, e.address, e.port].join("|"); });
// promisify
function cbStyle(a, b, cb) { setImmediate(function () { cb(null, a + b); }); }
function cbMulti(cb) { cb(null, 1, 2, 3); }
cbMulti[util.promisify.custom] = undefined;
function cbErr(cb) { cb(new Error("failed")); }
function cbNoArgs(cb) { cb(); }
function cbThis(cb) { cb(null, this && this.tag); }
var custom = function () {};
custom[util.promisify.custom] = function () { return Promise.resolve("custom!"); };
function withNames(cb) { cb(null, "x", "y"); }
withNames[Symbol.for("nodejs.util.promisify.custom")] = undefined;
attempt("promisify-bad", function () { return util.promisify("no"); });
attempt("promisify-custom-bad", function () { var f = function () {}; f[util.promisify.custom] = 5; return util.promisify(f); });
emit("promisify-custom-sym => " + (util.promisify.custom === Symbol.for("nodejs.util.promisify.custom")) + " " + String(util.promisify.custom));
emit("promisify-custom-same => " + (util.promisify(custom) === custom[util.promisify.custom]));
var pAdd = util.promisify(cbStyle);
emit("promisify-meta => " + [pAdd.name, pAdd.length, Object.getPrototypeOf(pAdd) === Function.prototype, typeof pAdd[util.promisify.custom]].join());
var asyncResults = [];
function later(label, p) {
  p.then(function (v) { asyncResults.push(label + " ok " + inspect(v)); }, function (e) { asyncResults.push(label + " err " + e.message + " " + (e.code || "")); });
}
later("p-add", pAdd(1, 2));
later("p-multi", util.promisify(cbMulti)());
later("p-err", util.promisify(cbErr)());
later("p-noargs", util.promisify(cbNoArgs)());
later("p-this", util.promisify(cbThis).call({ tag: "T" }));
later("p-custom", util.promisify(custom)());
later("p-names", (function () {
  var f = function (cb) { cb(null, "a", "b"); };
  f[Symbol("customPromisifyArgs")] = ["first", "second"];
  return util.promisify(f)();
})());
// callbackify
function asyncOk() { return Promise.resolve("ok"); }
function asyncFail() { return Promise.reject(new Error("rejected")); }
function asyncFalsy() { return Promise.reject(null); }
function asyncArgs(a, b) { return Promise.resolve(a + b); }
attempt("callbackify-bad", function () { return util.callbackify(5); });
attempt("callbackify-nocb", function () { return util.callbackify(asyncOk)(); });
attempt("callbackify-meta", function () { var f = util.callbackify(asyncArgs); return [f.name, f.length].join(); });
util.callbackify(asyncOk)(function (e, v) { asyncResults.push("cb-ok " + e + " " + v); });
util.callbackify(asyncFail)(function (e, v) { asyncResults.push("cb-fail " + e.message + " " + v); });
util.callbackify(asyncFalsy)(function (e, v) { asyncResults.push("cb-falsy " + e.code + " " + e.message + " " + e.reason + " " + (e instanceof Error)); });
util.callbackify(asyncArgs)(2, 3, function (e, v) { asyncResults.push("cb-args " + e + " " + v); });
// deprecate (warnings are disabled globally; the wrapper must still work)
var dep = util.deprecate(function (a) { return "dep:" + a; }, "msg", "DEP9999");
emit("deprecate => " + dep(1) + " " + dep.name + " " + dep.length);
attempt("deprecate-bad-code", function () { return util.deprecate(function () {}, "m", 5); });
attempt("deprecate-bad-fn", function () { try { util.deprecate(5, "m"); } catch (e) { return e.name; } });
var DepClass = util.deprecate(class DC { constructor(x) { this.x = x; } }, "class dep", "DEP9998");
emit("deprecate-class => " + new DepClass(5).x + " " + (new DepClass(1) instanceof DepClass));
// isDeepStrictEqual
function ds(label, a, b) { attempt("deep-" + label, function () { return [util.isDeepStrictEqual(a, b), util.isDeepStrictEqual(b, a)].join(); }); }
ds("prims", 1, 1);
ds("prims-ne", 1, "1");
ds("zero", 0, -0);
ds("nan", NaN, NaN);
ds("arr", [1, [2, 3]], [1, [2, 3]]);
ds("arr-ne", [1, 2], [1, 2, 3]);
ds("sparse", [1, , 3], [1, undefined, 3]);
ds("obj", { a: 1, b: { c: [1] } }, { b: { c: [1] }, a: 1 });
ds("obj-proto", { a: 1 }, Object.create({}, { a: { value: 1, enumerable: true } }));
ds("obj-nullproto", Object.create(null), {});
ds("class", new Parent(), new Parent());
ds("class-ne", new Parent(), new Child());
ds("date", new Date(5), new Date(5));
ds("date-ne", new Date(5), new Date(6));
ds("regexp", /a/g, /a/g);
ds("regexp-ne", /a/g, /a/i);
ds("regexp-last", Object.assign(/a/g, { lastIndex: 1 }), /a/g);
ds("map", new Map([[1, { a: 1 }]]), new Map([[1, { a: 1 }]]));
ds("map-ne", new Map([[1, 2]]), new Map([[1, 3]]));
ds("map-order", new Map([[1, 1], [2, 2]]), new Map([[2, 2], [1, 1]]));
ds("map-objkey", new Map([[{ k: 1 }, 1]]), new Map([[{ k: 1 }, 1]]));
ds("set", new Set([1, { a: 1 }]), new Set([{ a: 1 }, 1]));
ds("set-ne", new Set([1]), new Set([2]));
ds("typed", new Uint8Array([1, 2]), new Uint8Array([1, 2]));
ds("typed-ne", new Uint8Array([1, 2]), new Int8Array([1, 2]));
ds("typed-float", new Float64Array([NaN, 0]), new Float64Array([NaN, -0]));
ds("buffer", new ArrayBuffer(2), new ArrayBuffer(2));
ds("buffer-ne", Uint8Array.of(1).buffer, Uint8Array.of(2).buffer);
ds("error", fakeError(Error, "a"), fakeError(Error, "a"));
ds("error-ne", fakeError(Error, "a"), fakeError(Error, "b"));
ds("error-name", fakeError(TypeError, "a"), fakeError(Error, "a"));
ds("boxed", new Number(1), new Number(1));
ds("boxed-ne", new Number(1), new Number(2));
ds("boxed-str", new String("a"), "a");
ds("symbols", { [Symbol.for("s")]: 1 }, { [Symbol.for("s")]: 1 });
ds("symbols-ne", { [Symbol.for("s")]: 1 }, { [Symbol.for("s")]: 2 });
ds("circular", (function () { var a = {}; a.a = a; return a; })(), (function () { var a = {}; a.a = a; return a; })());
ds("circular-ne", (function () { var a = { x: 1 }; a.a = a; return a; })(), (function () { var a = { x: 2 }; a.a = a; return a; })());
ds("nested-ne", { a: { b: { c: 1 } } }, { a: { b: { c: 2 } } });
ds("fn", function () {}, function () {});
ds("fn-same", Parent, Parent);
ds("promise", Promise.resolve(1), Promise.resolve(1));
ds("weak", new WeakMap(), new WeakMap());
ds("args", (function () { return arguments; })(1), (function () { return arguments; })(1));
ds("args-arr", (function () { return arguments; })(1), [1]);
attempt("deep-skipproto", function () { return util.isDeepStrictEqual({ a: 1 }, Object.create(null, { a: { value: 1, enumerable: true } }), true); });
ds("big-arrays", Array.from({ length: 500 }, function (_, i) { return { i: i }; }), Array.from({ length: 500 }, function (_, i) { return { i: i }; }));
// parseArgs
function pa(label, config) {
  attempt("parseArgs-" + label, function () { var r = util.parseArgs(config); return JSON.stringify(r); });
}
pa("basic", { args: ["-f", "--bar", "b"], options: { foo: { type: "boolean", short: "f" }, bar: { type: "string" } } });
pa("positionals", { args: ["a", "--x", "b", "c"], options: { x: { type: "boolean" } }, allowPositionals: true });
pa("unknown", { args: ["--nope"], options: {} });
pa("unknown-nonstrict", { args: ["--nope"], options: {}, strict: false });
pa("positional-strict", { args: ["pos"], options: {} });
pa("multiple", { args: ["--a=1", "--a=2"], options: { a: { type: "string", multiple: true } } });
pa("default", { args: [], options: { a: { type: "string", default: "dflt" }, b: { type: "boolean", default: true } } });
pa("short-group", { args: ["-abc"], options: { a: { type: "boolean", short: "a" }, b: { type: "boolean", short: "b" }, c: { type: "boolean", short: "c" } } });
pa("tokens", { args: ["-f", "--bar=x", "pos", "--", "--rest"], options: { f: { type: "boolean", short: "f" }, bar: { type: "string" } }, allowPositionals: true, tokens: true });
pa("negative", { args: ["--no-color"], options: { color: { type: "boolean" } }, allowNegative: true });
pa("inline-value", { args: ["--str=a=b"], options: { str: { type: "string" } } });
pa("missing-value", { args: ["--str"], options: { str: { type: "string" } } });
pa("ambiguous", { args: ["--str", "--other"], options: { str: { type: "string" }, other: { type: "boolean" } } });
pa("bad-type", { args: [], options: { a: { type: "number" } } });
pa("bad-short", { args: [], options: { a: { type: "boolean", short: "ab" } } });
pa("bad-config", { args: "x" });
pa("bool-with-value", { args: ["--flag=yes"], options: { flag: { type: "boolean" } } });
pa("dashdash", { args: ["--", "--x"], options: { x: { type: "boolean" } }, allowPositionals: true });
pa("null-proto-values", { args: ["--toString"], options: { toString: { type: "boolean" } } });
// styleText
attempt("styleText", function () { return JSON.stringify([util.styleText("red", "x", { validateStream: false }), util.styleText(["bold", "underline"], "y", { validateStream: false }), util.styleText("none", "z", { validateStream: false })]); });
attempt("styleText-nested", function () { return JSON.stringify(util.styleText("red", "a\u001b[39mb", { validateStream: false })); });
attempt("styleText-hex", function () { return JSON.stringify([util.styleText("#ff8000", "h", { validateStream: false }), util.styleText("#f80", "s", { validateStream: false })]); });
attempt("styleText-bad", function () { return util.styleText("nocolor", "x", { validateStream: false }); });
attempt("styleText-bad-hex", function () { return util.styleText("#12", "x", { validateStream: false }); });
attempt("styleText-bad-text", function () { return util.styleText("red", 5, { validateStream: false }); });
attempt("styleText-stream", function () { return JSON.stringify(util.styleText("red", "x", { stream: { isTTY: false, write: function () {} } })); });
attempt("styleText-badstream", function () { return util.styleText("red", "x", { stream: 5 }); });
// MIMEType
attempt("mime", function () { var m = new util.MIMEType("text/html; charset=UTF-8; q=\"a b\""); return [m.type, m.subtype, m.essence, String(m), m.params.get("charset"), m.params.get("q"), m.params.has("x"), JSON.stringify(Array.from(m.params))].join("|"); });
attempt("mime-set", function () { var m = new util.MIMEType("text/plain"); m.type = "Application"; m.subtype = "JSON"; m.params.set("a", "b c"); return String(m) + " " + JSON.stringify(m); });
attempt("mime-bad", function () { return new util.MIMEType("nonsense"); });
attempt("mime-bad2", function () { return new util.MIMEType("text/"); });
attempt("mime-bad-set", function () { var m = new util.MIMEType("a/b"); m.type = ""; });
attempt("mime-params", function () { var p = new util.MIMEParams(); p.set("x", "1"); p.set("y", "2"); p.delete("x"); return [String(p), JSON.stringify(Array.from(p.keys())), JSON.stringify(Array.from(p.values())), p.size].join("|"); });
attempt("mime-inspect", function () { return inspect(new util.MIMEType("text/html;a=b")); });
attempt("mime-quoted", function () { return String(new util.MIMEType('a/b;x="q\\"uoted";y="semi;colon"')); });
attempt("mime-case", function () { var m = new util.MIMEType("TEXT/HTML;CHARSET=UTF-8"); return String(m); });
// debuglog
attempt("debuglog", function () { var d = util.debuglog("corpus-section"); return [typeof d, d.enabled, typeof util.debug].join(); });
attempt("debuglog-cb", function () { var seen; var d = util.debuglog("corpus-cb", function (fn) { seen = typeof fn; }); d("x"); return seen; });
// TextEncoder / TextDecoder
attempt("textenc", function () { return JSON.stringify(Array.from(new util.TextEncoder().encode("hé€😀"))); });
attempt("textdec", function () { return new util.TextDecoder().decode(new Uint8Array([104, 195, 169, 226, 130, 172])); });
attempt("text-same", function () { return util.TextEncoder === globalThis.TextEncoder && util.TextDecoder === globalThis.TextDecoder; });
// parseEnv
attempt("parseEnv", function () { return JSON.stringify(util.parseEnv("A=1\nB='two words'\nC=\"quoted # hash\"\n# comment\nD=plain # trailing\nexport E=exp\nF=\nG=\"line\\nbreak\"\n\nH = spaced ")); });
attempt("parseEnv-bad", function () { return util.parseEnv(5); });
// getCallSites exists
attempt("callsites", function () { return typeof util.getCallSites; });
// transferable / aborted exist
attempt("transferable", function () { var c = util.transferableAbortController(); return [c instanceof AbortController, util.transferableAbortSignal(c.signal) === c.signal].join(); });
attempt("transferable-bad", function () { return util.transferableAbortSignal({}); });
attempt("abort-helpers", function () { return [typeof util.aborted, typeof util.transferableAbortSignal, typeof util.transferableAbortController].join(); });
attempt("toUSV-symbol", function () { return util.toUSVString(Symbol("x")); });
// isDeepStrictEqual with getters / extra props
ds("props", Object.assign([1, 2], { extra: 1 }), [1, 2]);
ds("props-same", Object.assign([1, 2], { extra: 1 }), Object.assign([1, 2], { extra: 1 }));
ds("url-like", { href: "http://a/", protocol: "http:" }, { href: "http://a/", protocol: "http:" });
Promise.resolve().then(function () { return new Promise(function (r) { setTimeout(r, 20); }); }).then(function () {
  asyncResults.forEach(function (l) { emit("async " + l); });
  process.stdout.write(out.join("\n") + "\n");
  process.stdout.write("digest " + hash.toString(16) + " " + out.length + "\n");
});
