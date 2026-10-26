// Map, Set, Weak collections, iterators, promises, proxies, custom inspect
var m = new Map([[1, "one"], ["two", 2], [{ k: 1 }, [1, 2]], [null, undefined]]);
var s = new Set([1, "a", { x: 1 }, [1, 2], null]);
var bigMap = new Map();
for (var i = 0; i < 120; i++) bigMap.set("k" + i, i);
var bigSet = new Set();
for (var j = 0; j < 120; j++) bigSet.add(j);
var mapExtra = new Map([[1, 2]]);
mapExtra.extra = "prop";
var setExtra = new Set([1]);
setExtra.extra = { a: 1 };
class MyMap extends Map {}
class MySet extends Set {}
var nestedMap = new Map([["a", new Map([["b", new Map([["c", new Map([["d", 1]])]])]])]]);
showAll("coll", [new Map(), new Set(), m, s, bigMap, bigSet, mapExtra, setExtra, new MyMap([[1, 2]]), new MySet([1]), nestedMap,
  new Map([[new Map(), new Set()]]), new Set([new Set([new Set([1])])]), new Map([[NaN, -0]])]);
showAll("coll-depth0", [m, s, nestedMap], { depth: 0 });
showAll("coll-depth1", [nestedMap], { depth: 1 });
showAll("coll-opt", [bigMap, bigSet], { maxArrayLength: 3 });
showAll("coll-compact", [m, s], { compact: false });
showAll("coll-sorted", [new Map([["b", 1], ["a", 2]]), new Set(["z", "a"])], { sorted: true });
showAll("coll-hidden", [m, new Set([1])], { showHidden: true });
showAll("coll-colors", [m, s], { colors: true });
showAll("coll-break", [m, s], { breakLength: 20 });
// iterators
var mi = m.entries();
mi.next();
var si = s.values();
si.next();
si.next();
var emptyIter = new Set().values();
var doneIter = new Set([1]).values();
doneIter.next();
doneIter.next();
showAll("iter", [m.keys(), m.values(), m.entries(), s.values(), s.entries(), mi, si, emptyIter, doneIter, m[Symbol.iterator](), s[Symbol.iterator]()]);
showAll("iter-depth0", [m.entries(), s.entries()], { depth: 0 });
showAll("iter-hidden", [m.entries(), s.values()], { showHidden: true });
// weak collections
var wk1 = {}, wk2 = [1];
var wm = new WeakMap([[wk1, 1], [wk2, "two"]]);
var ws = new WeakSet([wk1, wk2]);
showAll("weak", [new WeakMap(), new WeakSet(), wm, ws]);
showAll("weak-hidden", [wm, ws, new WeakMap()], { showHidden: true });
show("weak-nested", { w: wm, s: [ws] });
// promises
var pPending = new Promise(function () {});
var pResolved = Promise.resolve({ a: 1 });
var pRejected = Promise.reject(fakeError(Error, "rejected"));
pRejected.catch(function () {});
var pObjRejected = Promise.reject({ reason: 1 });
pObjRejected.catch(function () {});
var pExtra = Promise.resolve(1);
pExtra.extra = "x";
class MyPromise extends Promise {}
var pSub = MyPromise.resolve(1);
var pNested = Promise.resolve(Promise.resolve([1, 2]));
var pPlain = Promise.resolve("str");
showAll("promise", [pPending, pResolved, pRejected, pObjRejected, pExtra, pSub, pNested, pPlain, Promise.resolve(undefined), Promise.resolve(null)]);
showAll("promise-depth0", [pResolved, pPending], { depth: 0 });
showAll("promise-hidden", [pResolved], { showHidden: true });
show("promise-in-obj", { p: pPending, q: [pResolved] });
// proxies
var target = { a: 1, nested: { b: 2 } };
var handler = { get: function () { throw new Error("trap must not run"); }, ownKeys: function () { throw new Error("trap must not run"); } };
var proxy = new Proxy(target, handler);
var fnProxy = new Proxy(function named() {}, {});
var arrProxy = new Proxy([1, 2, 3], {});
var revocable = Proxy.revocable({}, {});
revocable.revoke();
var proxyOfProxy = new Proxy(proxy, {});
showAll("proxy", [proxy, fnProxy, arrProxy, revocable.proxy, proxyOfProxy, new Proxy(new Map([[1, 2]]), {}), new Proxy(class K {}, {}), new Proxy(new Date(0), {}), new Proxy(/x/, {}), new Proxy(fakeError(Error, "pe"), {})]);
showAll("proxy-show", [proxy, fnProxy, arrProxy, revocable.proxy, proxyOfProxy], { showProxy: true });
showAll("proxy-show-depth0", [proxy], { showProxy: true, depth: 0 });
show("proxy-nested", { p: proxy, list: [arrProxy] });
show("proxy-circular", (function () { var o = {}; var p = new Proxy(o, {}); o.me = p; return p; })());
// custom inspect
var customStr = { [inspect.custom]() { return "CUSTOM"; } };
var customObj = { [inspect.custom]() { return { replaced: true }; } };
var customArgs = { [inspect.custom](depth, options, insp) { return "depth=" + depth + " keys=" + Object.keys(options).sort().join(",") + " same=" + (insp === inspect); } };
var customNested = { a: { [inspect.custom](depth) { return "nested@" + depth; } }, b: [{ [inspect.custom]() { return "in array"; } }] };
var customMulti = { [inspect.custom]() { return "line1\nline2"; } };
var customThrows = { [inspect.custom]() { throw new Error("custom failed"); } };
var customSelf = { [inspect.custom]() { return this; } };
var customNull = { [inspect.custom]: null };
var customNotFn = { [inspect.custom]: 5 };
var customUndefined = { [inspect.custom]() {} };
var customOptions = { [inspect.custom](depth, options) { return options.stylize("styled", "string"); } };
var customInspect = { inspect() { return "old-style"; } };
var customLong = { [inspect.custom]() { return "x".repeat(200); } };
var customPrim = { [inspect.custom]() { return 42; } };
var customInherit = Object.create({ [inspect.custom]() { return "from proto"; } });
class CustomClass { [inspect.custom](d, o) { return "CustomClass<" + d + ">"; } }
var customSymFor = { [Symbol.for("nodejs.util.inspect.custom")]() { return "via Symbol.for"; } };
showAll("custom", [customStr, customObj, customArgs, customNested, customMulti, customSelf, customNull, customNotFn, customUndefined, customOptions,
  customInspect, customLong, customPrim, customInherit, new CustomClass(), customSymFor, [customStr], { x: customStr }, new Map([[customStr, customStr]])]);
attempt("custom-throws-call", function () { return inspect(customThrows); });
showAll("custom-colors", [customOptions, customNested], { colors: true });
showAll("custom-opts", [customArgs], { depth: 5, showHidden: true, colors: true, compact: 1, breakLength: 50, sorted: true });
show("custom-depth-pass", { l1: { l2: { l3: customArgs } } }, { depth: 4 });
// stylize / inspect.defaultOptions / legacy signature
show("legacy-args", { a: { b: { c: {} } } }, true);
attempt("legacy-multi", function () { return inspect({ a: { b: { c: {} } } }, false, 0, true); });
attempt("legacy-multi2", function () { return inspect({ a: { b: { c: {} } } }, true, null, false); });
attempt("opts-null", function () { return inspect({ a: 1 }, null); });
attempt("opts-bad-depth", function () { return inspect({ a: { b: 1 } }, { depth: "x" }); });
attempt("defaults-keys", function () { return Object.keys(inspect.defaultOptions).join(","); });
attempt("defaults-set", function () {
  var old = inspect.defaultOptions.depth;
  inspect.defaultOptions = { depth: 0 };
  var r = inspect({ a: { b: 1 } });
  inspect.defaultOptions = { depth: old };
  return r;
});
attempt("defaults-bad", function () { inspect.defaultOptions = "no"; });
attempt("colors-keys", function () { return Object.keys(inspect.colors).join(","); });
attempt("styles", function () { return JSON.stringify(Object.assign({}, inspect.styles, { regexp: "fn" })); });
attempt("custom-symbol", function () { return String(inspect.custom); });
