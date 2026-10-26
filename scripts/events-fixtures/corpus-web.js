// Second half of the events corpus (EventTarget, AbortSignal, runner); the
// oracle and tests/node_events.rs run corpus-emitter.js followed by this file.

function eventTarget() {
  var et = new EventTarget();
  var log = [];
  et.addEventListener("x", function a(ev) { log.push("a:" + ev.type + ":" + (ev.target === et) + ":" + (ev.currentTarget === et) + ":" + ev.eventPhase); });
  et.addEventListener("x", { handleEvent: function (ev) { log.push("obj:" + (this !== et)); } });
  et.addEventListener("x", function once(ev) { log.push("once"); }, { once: true });
  var dup = function dup() { log.push("dup"); };
  et.addEventListener("x", dup); et.addEventListener("x", dup);
  var ev = new Event("x", { cancelable: true });
  out("dispatch", et.dispatchEvent(ev), log, ev.eventPhase, ev.target === et, ev.currentTarget);
  log.length = 0;
  et.dispatchEvent(new Event("x"));
  out("second dispatch", log);
  var ev2 = new Event("p", { cancelable: true });
  et.addEventListener("p", function (e) { e.preventDefault(); });
  out("preventDefault", et.dispatchEvent(ev2), ev2.defaultPrevented, ev2.cancelable);
  var ev3 = new Event("p");
  out("not cancelable", et.dispatchEvent(ev3), ev3.defaultPrevented);
  var stops = [];
  et.addEventListener("s", function (e) { stops.push(1); e.stopImmediatePropagation(); });
  et.addEventListener("s", function () { stops.push(2); });
  et.dispatchEvent(new Event("s"));
  out("stopImmediatePropagation", stops);
  var ac = new AbortController();
  var sigLog = [];
  et.addEventListener("sig", function () { sigLog.push("sig"); }, { signal: ac.signal });
  et.dispatchEvent(new Event("sig")); ac.abort(); et.dispatchEvent(new Event("sig"));
  out("signal option", sigLog);
  var rm = function () { log.push("rm"); };
  et.addEventListener("r", rm); et.removeEventListener("r", rm); et.removeEventListener("r", rm);
  log.length = 0; et.dispatchEvent(new Event("r"));
  out("removeEventListener", log);
  out("Event", (function () {
    var e = new Event("t", { bubbles: true, composed: true });
    return [e.type, e.bubbles, e.cancelable, e.composed, e.defaultPrevented, e.isTrusted, e.eventPhase, e.target, e.currentTarget, e.srcElement,
      e.returnValue, e.cancelBubble, typeof e.timeStamp, Event.NONE, Event.CAPTURING_PHASE, Event.AT_TARGET, Event.BUBBLING_PHASE, e.composedPath().length,
      Object.prototype.toString.call(e)];
  })());
  out("CustomEvent", (function () {
    var c = new CustomEvent("c", { detail: { a: 1 } });
    return [c.type, c.detail, new CustomEvent("d").detail, c instanceof Event, Object.prototype.toString.call(c)];
  })());
  attempt("Event no args", function () { return new Event(); });
  attempt("Event bad options", function () { return new Event("x", 5); });
  attempt("CustomEvent no args", function () { return new CustomEvent(); });
  attempt("addEventListener no args", function () { return et.addEventListener(); });
  attempt("addEventListener bad listener", function () { return et.addEventListener("x", 5); });
  attempt("addEventListener null listener", function () { return et.addEventListener("x", null); });
  attempt("dispatchEvent bad", function () { return et.dispatchEvent({}); });
  attempt("dispatchEvent no args", function () { return et.dispatchEvent(); });
  attempt("bad this", function () { return EventTarget.prototype.addEventListener.call({}, "x", f("x")); });
  attempt("bad this Event", function () { return Object.getOwnPropertyDescriptor(Event.prototype, "type").get.call({}); });
  var holder = new EventTarget();
  var hl = [];
  holder.dispatchEvent(new Event("ignored"));
  out("toStringTag", Object.prototype.toString.call(holder), Object.getOwnPropertyNames(EventTarget.prototype).sort(), Object.getOwnPropertyNames(Event.prototype).sort());
  class Sub extends EventTarget { constructor() { super(); this.onfoo = null; } }
  var s = new Sub();
  s.addEventListener("foo", function () { hl.push("sub"); });
  s.dispatchEvent(new Event("foo"));
  out("subclass", hl);
  var warnT = new EventTarget();
  EventEmitter.setMaxListeners(1, warnT);
  warnT.addEventListener("w", f("w1")); warnT.addEventListener("w", f("w2"));
  out("typeof globals", typeof EventTarget, typeof Event, typeof CustomEvent, typeof AbortController, typeof AbortSignal, typeof DOMException,
    Object.keys(globalThis).filter(function (k) { return /^(Event|EventTarget|CustomEvent|AbortController|AbortSignal|DOMException)$/.test(k); }));
}

function abort() {
  var ac = new AbortController();
  var seen = [];
  ac.signal.onabort = function (ev) { seen.push("onabort:" + ev.type + ":" + ev.isTrusted); };
  ac.signal.addEventListener("abort", function (ev) { seen.push("listener:" + (ev.target === ac.signal)); });
  out("before", ac.signal.aborted, ac.signal.reason, typeof ac.signal.throwIfAborted());
  ac.abort();
  var r = ac.signal.reason;
  out("after", ac.signal.aborted, r.name, r.code, r.message, r instanceof Error, r instanceof DOMException, seen);
  ac.abort("again");
  out("abort twice", ac.signal.reason === r);
  attempt("throwIfAborted", function () { ac.signal.throwIfAborted(); });
  var custom = new AbortController(); custom.abort("why");
  out("custom reason", custom.signal.reason);
  var undef = new AbortController(); undef.abort(undefined);
  out("undefined reason", undef.signal.reason && undef.signal.reason.name);
  var ns = new AbortController(); ns.abort(null);
  out("null reason", ns.signal.reason);
  out("AbortSignal.abort", AbortSignal.abort().aborted, AbortSignal.abort().reason.name, AbortSignal.abort("r").reason);
  attempt("new AbortSignal", function () { return new AbortSignal(); });
  out("tags", Object.prototype.toString.call(ac), Object.prototype.toString.call(ac.signal), ac.signal === ac.signal);
  out("props", Object.getOwnPropertyNames(AbortSignal.prototype).sort(), Object.getOwnPropertyNames(AbortController.prototype).sort(), Object.getOwnPropertyNames(AbortSignal).sort());
  attempt("signal bad this", function () { return Object.getOwnPropertyDescriptor(AbortSignal.prototype, "aborted").get.call({}); });
  attempt("controller bad this", function () { return Object.getOwnPropertyDescriptor(AbortController.prototype, "signal").get.call({}); });
  var d = new DOMException("msg", "DataCloneError");
  out("DOMException", d.name, d.code, d.message, d instanceof Error, String(d), Object.prototype.toString.call(d), DOMException.ABORT_ERR, DOMException.prototype.TIMEOUT_ERR,
    new DOMException().name, new DOMException("m").message, new DOMException("m", { name: "X", cause: 1 }).cause, new DOMException("m", "Nope").code,
    Object.getOwnPropertyNames(DOMException.prototype).sort(), Object.getPrototypeOf(DOMException.prototype) === Error.prototype);
  attempt("DOMException bad this", function () { return Object.getOwnPropertyDescriptor(DOMException.prototype, "name").get.call({}); });
}

function any() {
  var results = [];
  var a = new AbortController(), b = new AbortController();
  var combined = AbortSignal.any([a.signal, b.signal]);
  out("any initial", combined.aborted, combined.reason, combined instanceof AbortSignal);
  combined.addEventListener("abort", function () { results.push("combined fired"); });
  b.abort("b reason");
  out("any aborted", combined.aborted, combined.reason, a.signal.aborted, results);
  var pre = AbortController.prototype.constructor === AbortController && AbortSignal.abort("pre");
  var c = AbortSignal.any([new AbortController().signal, pre]);
  out("any already aborted", c.aborted, c.reason);
  out("any empty", AbortSignal.any([]).aborted);
  attempt("any bad", function () { return AbortSignal.any([1]); });
  attempt("any not iterable", function () { return AbortSignal.any(5); });
  attempt("any no args", function () { return AbortSignal.any(); });
  var chain1 = new AbortController();
  var mid = AbortSignal.any([chain1.signal]);
  var top = AbortSignal.any([mid]);
  var chainLog = [];
  top.addEventListener("abort", function () { chainLog.push("top"); });
  mid.addEventListener("abort", function () { chainLog.push("mid"); });
  chain1.abort("deep");
  out("any chain", top.aborted, top.reason, mid.reason, chainLog.sort());
  var set = new Set([new AbortController().signal]);
  out("any iterable", AbortSignal.any(set).aborted);
  var d1 = new AbortController();
  var dep = AbortSignal.any([d1.signal]);
  var order = [];
  d1.signal.addEventListener("abort", function () { order.push("source:" + dep.aborted); });
  dep.addEventListener("abort", function () { order.push("dependent:" + d1.signal.aborted); });
  d1.abort();
  out("any ordering", order);
}

function timeout() {
  attempt("timeout bad", function () { return AbortSignal.timeout(-1); });
  attempt("timeout str", function () { return AbortSignal.timeout("1"); });
  attempt("timeout none", function () { return AbortSignal.timeout(); });
  attempt("timeout NaN", function () { return AbortSignal.timeout(NaN); });
  var s = AbortSignal.timeout(10);
  var fired = [];
  s.addEventListener("abort", function (ev) { fired.push(ev.type); });
  out("timeout initial", s.aborted);
  var inAny = AbortSignal.any([AbortSignal.timeout(15), new AbortController().signal]);
  inAny.addEventListener("abort", function () { fired.push("any"); });
  return new Promise(function (r) { setTimeout(r, 60); }).then(function () {
    out("timeout fired", s.aborted, s.reason.name, s.reason.code, s.reason.message, fired);
    out("timeout in any", inAny.aborted, inAny.reason.name);
    var keepAlive = setInterval(function () {}, 5);
    return EventEmitter.once(new EventEmitter(), "never", { signal: AbortSignal.timeout(5) }).then(null, function (err) {
      clearInterval(keepAlive);
      out("once with timeout signal", err.name, err.cause && err.cause.name);
    });
  });
}

Promise.resolve()
  .then(basics).then(errors).then(ownProps).then(once).then(on).then(capture).then(statics)
  .then(eventTarget).then(abort).then(any).then(timeout)
  .then(function () {
    var digest = 0x811c9dc5;
    var text = lines.join("\n");
    for (var i = 0; i < text.length; i++) {
      digest = Math.imul(digest ^ text.charCodeAt(i), 0x01000193) >>> 0;
    }
    console.log(text);
    console.log("digest " + digest.toString(16));
    process.exit(0);
  }, function (err) {
    console.log(lines.join("\n"));
    console.log("FAILED", err && err.stack || err);
    process.exit(1);
  });
