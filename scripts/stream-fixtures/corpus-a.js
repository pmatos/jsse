// Deterministic differential corpus for the embedded stream module. Prints one
// line per observation plus a final digest line; the output must be
// byte-identical on Node and on jsse --node (scripts/run-node-stream-oracle.sh)
// and the digest is pinned in corpus.digest (tests/node_stream.rs). Scenarios
// log the order of events, callbacks, nextTicks and microtasks, so they pin
// backpressure and 'drain' ordering, not just final values. Values go through
// String and JSON, never util.inspect.
var stream = require("stream");
var Readable = stream.Readable;
var Writable = stream.Writable;
var Duplex = stream.Duplex;
var Transform = stream.Transform;
var PassThrough = stream.PassThrough;
var promises = require("stream/promises");

var lines = [];
function show(v) {
  if (typeof v === "string") return v;
  if (typeof v === "symbol") return String(v);
  if (typeof v === "function") return "[fn " + v.name + "]";
  if (v instanceof Error) return v.name + "|" + v.code + "|" + v.message;
  if (v && typeof v === "object" && typeof v.pipe === "function") return "[stream]";
  if (v instanceof Uint8Array) return "<" + Array.prototype.join.call(v, ",") + ">";
  try { return JSON.stringify(v, function (k, x) { return typeof x === "symbol" ? String(x) : x; }); } catch (e) { return String(v); }
}
function out() {
  lines.push(Array.prototype.map.call(arguments, show).join(" "));
}
function attempt(label, fn) {
  try { out(label, "->", fn()); } catch (e) { out(label, "throws", e); }
}
function watch(label, s, names) {
  names.forEach(function (name) {
    s.on(name, function () {
      out(label, name, Array.prototype.map.call(arguments, show).join(","));
    });
  });
}
var scenarios = [];
function scenario(name, fn) { scenarios.push([name, fn]); }
function tick(label) { process.nextTick(function () { out("tick", label); }); }
function micro(label) { Promise.resolve().then(function () { out("micro", label); }); }
function immediate() { return new Promise(function (resolve) { setImmediate(resolve); }); }
function closed(s) { return new Promise(function (resolve) { s.on("close", resolve); }); }
var FLOW_EVENTS = ["pipe", "unpipe", "data", "end", "finish", "prefinish", "close", "error", "drain", "pause", "resume", "readable"];

scenario("writable backpressure sync", function () {
  var w = new Writable({
    highWaterMark: 3,
    write: function (chunk, enc, cb) { out("_write", chunk, enc); cb(); },
  });
  watch("w", w, ["drain", "finish", "close"]);
  out("write a", w.write("a"));
  out("write bcd", w.write("bcd"));
  out("needDrain", w.writableNeedDrain, w.writableLength, w.writableHighWaterMark);
  tick("after writes");
  w.end("e", function () { out("end cb"); });
  return closed(w);
});

scenario("writable backpressure async", function () {
  var pending = [];
  var w = new Writable({
    highWaterMark: 4,
    write: function (chunk, enc, cb) { out("_write", chunk); pending.push(cb); },
  });
  watch("w", w, ["drain", "finish", "close"]);
  out("w1", w.write("ab", function () { out("cb1"); }));
  out("w2", w.write("cd", function () { out("cb2"); }));
  out("w3", w.write("ef", function () { out("cb3"); }));
  out("len", w.writableLength, w.writableNeedDrain);
  pending.shift()();
  out("after first cb", w.writableLength, w.writableNeedDrain);
  pending.shift()();
  out("after second cb", w.writableLength);
  tick("t1");
  pending.shift()();
  return immediate().then(function () {
    out("final", w.writableLength, w.writableNeedDrain);
    w.end();
    return closed(w);
  });
});

scenario("writable cork writev", function () {
  var w = new Writable({
    writev: function (chunks, cb) {
      out("_writev", chunks.map(function (c) { return [String(c.chunk), c.encoding]; }));
      cb();
    },
    write: function (chunk, enc, cb) { out("_write", String(chunk)); cb(); },
  });
  w.cork();
  w.write("a");
  w.write("b", "utf8");
  w.write(Buffer.from("c"));
  out("corked", w.writableCorked, w.writableLength);
  w.uncork();
  out("uncorked", w.writableCorked);
  w.cork(); w.cork();
  w.write("d");
  w.uncork();
  out("still", w.writableCorked);
  w.uncork();
  w.end();
  return closed(w);
});

scenario("writable decodeStrings and defaultEncoding", function () {
  var w = new Writable({
    decodeStrings: false,
    write: function (chunk, enc, cb) { out("_write", typeof chunk, chunk, enc); cb(); },
  });
  w.write("x");
  w.write("6162", "hex");
  w.setDefaultEncoding("latin1");
  w.write("y");
  attempt("setDefaultEncoding bad", function () { return w.setDefaultEncoding("nope"); });
  out("defaultEncoding", w._writableState.defaultEncoding);
  w.end();
  return closed(w);
});

scenario("writable errors", function () {
  var w = new Writable({ write: function (c, e, cb) { cb(); } });
  watch("w", w, ["error", "finish", "close"]);
  attempt("write null", function () { return w.write(null); });
  attempt("write number", function () { return w.write(5); });
  w.end();
  var cbErr;
  w.write("late", function (err) { cbErr = err; });
  tick("t");
  return closed(w).then(function () { out("late write cb", cbErr); });
});

scenario("writable final and destroy", function () {
  var w = new Writable({
    write: function (c, e, cb) { out("_write", String(c)); cb(); },
    final: function (cb) { out("_final"); setImmediate(function () { out("final done"); cb(); }); },
    destroy: function (err, cb) { out("_destroy", err); cb(err); },
  });
  watch("w", w, ["prefinish", "finish", "close", "error"]);
  w.write("a");
  w.end();
  return closed(w).then(function () { out("flags", w.writableFinished, w.writableEnded, w.destroyed, w.closed); });
});

scenario("readable read(n) and readable event", function () {
  var r = new Readable({ highWaterMark: 4, read: function () {} });
  watch("r", r, ["readable", "end", "close"]);
  r.push("abc");
  out("read 2", String(r.read(2)));
  out("read 5", r.read(5));
  r.push("defg");
  out("len", r.readableLength, r.readableFlowing);
  out("read all", String(r.read()));
  out("read empty", r.read());
  r.push(null);
  tick("t");
  return closed(r.resume());
});

scenario("readable push return and highWaterMark", function () {
  var reads = 0;
  var r = new Readable({ highWaterMark: 2, read: function (n) { reads++; out("_read", n); } });
  out("push 1", r.push("a"));
  out("push 2", r.push("b"));
  out("push 3", r.push("c"));
  out("len", r.readableLength);
  out("read 1", String(r.read(1)));
  out("reads", reads);
  r.destroy();
  return closed(r);
});

scenario("readable unshift and pause resume", function () {
  var r = new Readable({ read: function () {} });
  watch("r", r, ["data", "end", "pause", "resume", "close"]);
  r.push("b");
  r.unshift("a");
  r.push("c");
  r.push(null);
  attempt("unshift after end", function () { r.unshift("z"); return "ok"; });
  out("paused", r.isPaused());
  r.on("data", function () {});
  out("flowing", r.readableFlowing, r.isPaused());
  r.pause();
  out("paused2", r.isPaused());
  r.resume();
  return closed(r);
});

scenario("readable objectMode", function () {
  var r = new Readable({ objectMode: true, highWaterMark: 2, read: function () {} });
  out("hwm", r.readableHighWaterMark, r.readableObjectMode);
  out("push obj", r.push({ a: 1 }));
  out("push undefined", r.push(undefined));
  out("push 0", r.push(0));
  out("len", r.readableLength);
  r.push(null);
  var seen = [];
  r.on("data", function (d) { seen.push(d); });
  return closed(r).then(function () { out("seen", seen); });
});

scenario("pipe backpressure", function () {
  var cbs = [];
  var src = new Readable({
    highWaterMark: 2,
    read: function () { out("src _read"); this.push(this.n < 5 ? String(this.n++) : null); },
  });
  src.n = 0;
  var dst = new Writable({
    highWaterMark: 1,
    write: function (c, e, cb) { out("dst _write", String(c)); setImmediate(cb); },
  });
  watch("src", src, ["pause", "resume", "end", "close"]);
  watch("dst", dst, ["pipe", "unpipe", "drain", "finish", "close"]);
  src.pipe(dst);
  return closed(dst);
});

scenario("pipe end false and unpipe", function () {
  var src = new Readable({ read: function () {} });
  var dst = new Writable({ write: function (c, e, cb) { out("dst", String(c)); cb(); } });
  watch("dst", dst, ["pipe", "unpipe", "finish"]);
  src.pipe(dst, { end: false });
  src.push("a");
  return immediate().then(function () {
    src.unpipe(dst);
    src.push("b");
    out("after unpipe", src.readableFlowing, src.readableLength);
    src.destroy();
  });
});

scenario("transform flush", function () {
  var t = new Transform({
    transform: function (chunk, enc, cb) { out("_transform", String(chunk)); cb(null, String(chunk).toUpperCase()); },
    flush: function (cb) { out("_flush"); this.push("!"); cb(); },
  });
  var got = [];
  t.on("data", function (d) { got.push(String(d)); });
  watch("t", t, ["finish", "end", "close"]);
  t.write("a");
  t.end("b");
  return closed(t).then(function () { out("got", got); });
});

scenario("transform errors", function () {
  var t = new Transform({ transform: function (c, e, cb) { cb(new Error("boom " + c)); } });
  watch("t", t, ["error", "close", "finish", "end"]);
  t.write("x");
  t.on("data", function () {});
  return closed(t);
});

scenario("passthrough chain", function () {
  var a = new PassThrough();
  var b = new PassThrough({ objectMode: true });
  a.pipe(b);
  var got = [];
  b.on("data", function (d) { got.push(String(d)); });
  a.write("1");
  a.end("2");
  return closed(b).then(function () { out("got", got); });
});

scenario("destroy event order", function () {
  var r = new Readable({ read: function () {}, destroy: function (e, cb) { out("_destroy", e); cb(e); } });
  watch("r", r, ["error", "close", "end"]);
  r.destroy(new Error("bye"));
  out("sync", r.destroyed, r.errored, r.closed);
  r.destroy(new Error("second"));
  return closed(r).then(function () { out("after", r.destroyed, r.closed, r.errored); });
});

scenario("emitClose false and autoDestroy false", function () {
  var r = new Readable({ read: function () {}, emitClose: false, autoDestroy: false });
  watch("r", r, ["end", "close"]);
  r.push(null);
  r.resume();
  return immediate().then(function () { out("destroyed", r.destroyed); r.destroy(); return immediate(); });
});

scenario("construct", function () {
  var order = [];
  var w = new Writable({
    construct: function (cb) { out("construct start"); setImmediate(function () { out("construct done"); cb(); }); },
    write: function (c, e, cb) { out("write", String(c)); cb(); },
  });
  out("constructed?", w._writableState.constructed);
  w.write("a");
  w.end();
  watch("w", w, ["finish", "close"]);
  return closed(w);
});
