// Deterministic differential corpus for the embedded events module, EventTarget
// and AbortSignal. Prints one line per observation plus a final digest line;
// the output must be byte-identical on Node and on jsse --node
// (scripts/run-node-events-oracle.sh), and the digest is pinned in
// corpus.digest (tests/node_events.rs). Values are printed through String and
// JSON rather than util.inspect, whose formatting is not part of this module.
var EventEmitter = typeof __node_builtin === "function" ? __node_builtin("events") : require("events");

var lines = [];
function out() {
  lines.push(Array.prototype.map.call(arguments, show).join(" "));
}
function show(v) {
  if (typeof v === "string") return v;
  if (typeof v === "symbol") return String(v);
  if (typeof v === "function") return "[fn " + v.name + "]";
  if (v instanceof Error) return v.name + "|" + v.code + "|" + v.message;
  try { return JSON.stringify(v, function (k, x) { return typeof x === "symbol" ? String(x) : x; }); } catch (e) { return String(v); }
}
function attempt(label, fn) {
  try { out(label, "->", fn()); } catch (e) { out(label, "throws", e); }
}
function f(name) { var fn = function () {}; Object.defineProperty(fn, "name", { value: name }); return fn; }

function basics() {
  var e = new EventEmitter();
  var log = [];
  e.on("a", function first(x) { log.push("first" + x); });
  e.prependListener("a", function pre(x) { log.push("pre" + x); });
  e.once("a", function once1(x) { log.push("once" + x); });
  e.prependOnceListener("a", function preonce(x) { log.push("preonce" + x); });
  out("emit1", e.emit("a", 1), log);
  log.length = 0;
  out("emit2", e.emit("a", 2), log);
  out("emit missing", e.emit("nope"));
  out("names", e.eventNames(), e.listenerCount("a"));
  var h = f("h");
  e.on("b", h); e.on("b", h);
  out("count b", e.listenerCount("b"), e.listenerCount("b", h), e.listenerCount("b", f("other")));
  e.off("b", h);
  out("after off", e.listenerCount("b"));
  e.once("c", h);
  out("rawListeners c", e.rawListeners("c").map(function (l) { return l.name + ":" + (l.listener === h); }));
  out("listeners c", e.listeners("c").map(function (l) { return l === h; }));
  var events = [];
  e.on("newListener", function (t, l) { events.push("new:" + String(t) + ":" + l.name); });
  e.on("removeListener", function (t, l) { events.push("rm:" + String(t) + ":" + l.name); });
  var sym = Symbol("s");
  e.on(sym, f("symListener"));
  e.removeAllListeners(sym);
  e.once("d", f("d1"));
  e.emit("d");
  out("meta events", events);
  e.removeAllListeners();
  out("after removeAll", e.eventNames(), events.length);
  out("chain", e.on("x", h) === e, e.once("x", h) === e, e.off("x", h) === e);
  out("max", e.getMaxListeners(), EventEmitter.defaultMaxListeners, e.setMaxListeners(3) === e, e.getMaxListeners());
  attempt("setMax -1", function () { return e.setMaxListeners(-1); });
  attempt("setMax str", function () { return e.setMaxListeners("1"); });
  attempt("default NaN", function () { EventEmitter.defaultMaxListeners = NaN; });
  attempt("on bad", function () { return e.on("x", 1); });
  attempt("once bad", function () { return e.once("x", {}); });
  attempt("off bad", function () { return e.off("x"); });
  out("exports", typeof EventEmitter.EventEmitter, EventEmitter.EventEmitter === EventEmitter, typeof EventEmitter.once, typeof EventEmitter.on,
    typeof EventEmitter.getEventListeners, typeof EventEmitter.setMaxListeners, typeof EventEmitter.addAbortListener,
    typeof EventEmitter.listenerCount, typeof EventEmitter.getMaxListeners, EventEmitter.usingDomains,
    String(EventEmitter.errorMonitor), String(EventEmitter.captureRejectionSymbol), EventEmitter.captureRejections);
  out("proto", Object.getOwnPropertyNames(EventEmitter.prototype).sort());
  out("statics", Object.keys(EventEmitter).filter(function (k) { return k !== "EventEmitterAsyncResource"; }).sort());
  var order = [];
  var em = new EventEmitter();
  em.on("t", function a() { order.push("a"); em.off("t", b); });
  function b() { order.push("b"); }
  em.on("t", b);
  em.emit("t"); em.emit("t");
  out("modify in emit", order);
  function Legacy() { EventEmitter.call(this); }
  Object.setPrototypeOf(Legacy.prototype, EventEmitter.prototype);
  var l = new Legacy(); var ctx;
  l.on("z", function () { ctx = this; });
  l.emit("z");
  out("this", ctx === l, l instanceof EventEmitter);
  out("args", (function () { var seen; var x = new EventEmitter(); x.on("n", function () { seen = arguments.length; }); x.emit("n", 1, 2, 3, 4, 5, 6, 7); return seen; })());
}

function errors() {
  var e = new EventEmitter();
  var boom = new Error("boom");
  attempt("error Error", function () { return e.emit("error", boom) && 1; });
  try { e.emit("error", boom); } catch (x) { out("same error", x === boom); }
  attempt("error string", function () { return e.emit("error", "str"); });
  attempt("error undefined", function () { return e.emit("error"); });
  attempt("error number", function () { return e.emit("error", 42); });
  attempt("error null", function () { return e.emit("error", null); });
  attempt("error plain", function () { return e.emit("error", { a: 1 }); });
  var m = new EventEmitter();
  var seq = [];
  m.on(EventEmitter.errorMonitor, function (err) { seq.push("monitor:" + err.message); });
  m.on("error", function (err) { seq.push("handler:" + err.message); });
  m.emit("error", new Error("e1"));
  out("errorMonitor with handler", seq);
  seq.length = 0;
  var m2 = new EventEmitter();
  m2.on(EventEmitter.errorMonitor, function (err) { seq.push("monitor:" + err.message); });
  try { m2.emit("error", new Error("e2")); } catch (x) { seq.push("thrown:" + x.message); }
  out("errorMonitor without handler", seq);
  var u = new EventEmitter();
  u.on("error", f("only"));
  out("error listener count", u.listenerCount("error"), u.emit("error", new Error("x")));
}

function ownProps() {
  var e = new EventEmitter();
  out("own", Object.getOwnPropertyNames(e).sort(), Reflect.ownKeys(e).filter(function (k) { return typeof k === "symbol"; }).length);
  out("init", e._events === Object.getPrototypeOf(e)._events, e._eventsCount, e._maxListeners);
  e.on("q", f("q"));
  out("after on", e._eventsCount, Object.keys(e._events), typeof e._events.q);
  e.on("q", f("q2"));
  out("two", Array.isArray(e._events.q), e._events.q.length);
}

function once(done) {
  var p = [];
  var e = new EventEmitter();
  var pr = EventEmitter.once(e, "ev");
  e.emit("ev", 1, 2, 3);
  p.push(pr.then(function (v) { out("once resolved", v, e.listenerCount("error"), e.listenerCount("ev")); }));
  var e2 = new EventEmitter();
  var pr2 = EventEmitter.once(e2, "ev");
  e2.emit("error", new Error("rejected"));
  p.push(pr2.then(function () { out("once error?"); }, function (err) { out("once rejects", err); }));
  var e3 = new EventEmitter();
  var pr3 = EventEmitter.once(e3, "error");
  e3.emit("error", new Error("as event"));
  p.push(pr3.then(function (v) { out("once error name", v.length, v[0].message); }));
  var ac = new AbortController();
  var e4 = new EventEmitter();
  var pr4 = EventEmitter.once(e4, "never", { signal: ac.signal });
  ac.abort();
  p.push(pr4.then(function () { out("once abort?"); }, function (err) {
    out("once aborted", err.name, err.code, err.message, err.cause === ac.signal.reason, e4.listenerCount("never"));
  }));
  var ac2 = new AbortController();
  ac2.abort();
  p.push(EventEmitter.once(new EventEmitter(), "x", { signal: ac2.signal }).then(null, function (err) { out("once pre-aborted", err.name, err.code); }));
  p.push(EventEmitter.once(new EventEmitter(), "x", 5).then(null, function (err) { out("once bad options", err); }));
  p.push(EventEmitter.once(new EventEmitter(), "x", { signal: {} }).then(null, function (err) { out("once bad signal", err); }));
  var et = new EventTarget();
  var pr5 = EventEmitter.once(et, "foo");
  et.dispatchEvent(new Event("foo"));
  p.push(pr5.then(function (v) { out("once EventTarget", v.length, v[0].type); }));
  p.push(EventEmitter.once({}, "x").then(null, function (err) { out("once bad emitter", err.name, err.code); }));
  return Promise.all(p);
}

function on() {
  var e = new EventEmitter();
  var it = EventEmitter.on(e, "data");
  e.emit("data", 1, 2); e.emit("data", 3);
  var got = [];
  return it.next().then(function (r) {
    got.push(r);
    return it.next();
  }).then(function (r) {
    got.push(r);
    var pending = it.next();
    e.emit("data", "late");
    return pending;
  }).then(function (r) {
    got.push(r);
    return it.return();
  }).then(function (r) {
    got.push(r);
    out("on results", got, e.listenerCount("data"), e.listenerCount("error"));
    var e2 = new EventEmitter();
    var it2 = EventEmitter.on(e2, "data");
    e2.emit("error", new Error("iter err"));
    return it2.next().then(function () { out("on error?"); }, function (err) { out("on rejects", err); return it2.next(); });
  }).then(function (r) {
    out("on after error", r);
    var e3 = new EventEmitter();
    var it3 = EventEmitter.on(e3, "x", { close: ["end"] });
    e3.emit("x", 1);
    e3.emit("end");
    return it3.next().then(function (a) { return it3.next().then(function (b) { out("on close", a, b); }); });
  }).then(function () {
    var e4 = new EventEmitter();
    var ac = new AbortController();
    var it4 = EventEmitter.on(e4, "x", { signal: ac.signal });
    var pending = it4.next();
    ac.abort();
    return pending.then(function () { out("on abort?"); }, function (err) { out("on aborted", err.name, err.code); });
  }).then(function () {
    var e5 = new EventEmitter();
    var calls = [];
    e5.pause = function () { calls.push("pause"); };
    e5.resume = function () { calls.push("resume"); };
    var it5 = EventEmitter.on(e5, "x", { highWaterMark: 2, lowWaterMark: 1 });
    e5.emit("x", 1); e5.emit("x", 2); e5.emit("x", 3);
    return it5.next().then(function () { return it5.next(); }).then(function () { return it5.return(); }).then(function () { out("on watermarks", calls); });
  }).then(function () {
    out("on bad", (function () { try { EventEmitter.on(new EventEmitter(), "x", { highWaterMark: 0 }); } catch (err) { return err; } })());
    var it6 = EventEmitter.on(new EventEmitter(), "x");
    out("on throw", (function () { try { it6.throw("no"); } catch (err) { return err; } })());
    out("on iterator", it6[Symbol.asyncIterator]() === it6, Object.prototype.toString.call(it6));
    var et = new EventTarget();
    var it7 = EventEmitter.on(et, "tick");
    et.dispatchEvent(new Event("tick"));
    return it7.next().then(function (r) { out("on EventTarget", r.done, r.value.length, r.value[0].type); return it7.return(); });
  });
}

function capture() {
  var ee = new EventEmitter({ captureRejections: true });
  var steps = [];
  ee.on("error", function (err) { steps.push("error:" + err.message); });
  ee.on("evt", function () { return Promise.reject(new Error("rejected listener")); });
  ee.emit("evt");
  var custom = new EventEmitter({ captureRejections: true });
  custom[Symbol.for("nodejs.rejection")] = function (err, type, a) { steps.push("custom:" + err.message + ":" + type + ":" + a); };
  custom.on("evt", async function () { throw new Error("async throw"); });
  custom.emit("evt", "arg");
  var off = new EventEmitter();
  off.on("error", function (err) { steps.push("never:" + err.message); });
  off.on("evt", function () { return Promise.reject(new Error("ignored")).catch(function () {}); });
  off.emit("evt");
  attempt("captureRejections bad", function () { return new EventEmitter({ captureRejections: "yes" }); });
  attempt("captureRejections global bad", function () { EventEmitter.captureRejections = 1; });
  var prev = EventEmitter.captureRejections;
  EventEmitter.captureRejections = true;
  var g = new EventEmitter();
  EventEmitter.captureRejections = prev;
  g.on("error", function (err) { steps.push("global:" + err.message); });
  g.on("evt", function () { return Promise.reject(new Error("g")); });
  g.emit("evt");
  return new Promise(function (r) { setTimeout(r, 30); }).then(function () { out("capture steps", steps.sort()); });
}

function statics() {
  var e = new EventEmitter();
  var h1 = f("h1"), h2 = f("h2");
  e.on("a", h1); e.once("a", h2);
  out("getEventListeners EE", EventEmitter.getEventListeners(e, "a").map(function (l) { return l.name; }));
  var et = new EventTarget();
  et.addEventListener("a", h1); et.addEventListener("a", h2); et.addEventListener("b", h2);
  out("getEventListeners ET", EventEmitter.getEventListeners(et, "a").map(function (l) { return l.name; }), EventEmitter.getEventListeners(et, "none").length);
  attempt("getEventListeners bad", function () { return EventEmitter.getEventListeners({}, "a"); });
  out("getMaxListeners", EventEmitter.getMaxListeners(e), EventEmitter.getMaxListeners(et));
  attempt("getMaxListeners bad", function () { return EventEmitter.getMaxListeners({}); });
  EventEmitter.setMaxListeners(7, e, et);
  out("setMaxListeners", e.getMaxListeners(), EventEmitter.getMaxListeners(et));
  attempt("setMaxListeners bad", function () { return EventEmitter.setMaxListeners(1, {}); });
  attempt("setMaxListeners neg", function () { return EventEmitter.setMaxListeners(-1); });
  out("listenerCount static", EventEmitter.listenerCount(e, "a"), EventEmitter.listenerCount(et, "a"));
  var ac = new AbortController();
  var fired = [];
  var disp = EventEmitter.addAbortListener(ac.signal, function (ev) { fired.push(ev.type); });
  out("addAbortListener", typeof disp[Symbol.dispose], fired);
  ac.abort();
  var ac2 = new AbortController();
  var d2 = EventEmitter.addAbortListener(ac2.signal, function () { fired.push("never"); });
  d2[Symbol.dispose]();
  ac2.abort();
  attempt("addAbortListener bad", function () { return EventEmitter.addAbortListener(undefined, f("x")); });
  attempt("addAbortListener bad listener", function () { return EventEmitter.addAbortListener(new AbortController().signal, 1); });
  var pre = new AbortController(); pre.abort();
  EventEmitter.addAbortListener(pre.signal, function () { fired.push("pre"); });
  fired.push("sync");
  return Promise.resolve().then(function () { return Promise.resolve(); }).then(function () { out("abort listeners fired", fired); });
}
