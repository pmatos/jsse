scenario("Readable.from", function () {
  var got = [];
  var r = Readable.from(["a", "b", { c: 1 }]);
  out("objectMode", r.readableObjectMode, r.readableHighWaterMark);
  r.on("data", function (d) { got.push(d); });
  var s = Readable.from("hello");
  out("string objectMode", s.readableObjectMode);
  var b = Readable.from(Buffer.from("xy"), { objectMode: false });
  var gotB = [];
  b.on("data", function (d) { gotB.push(String(d)); });
  attempt("from number", function () { Readable.from(5); });
  attempt("from null", function () { Readable.from(null); });
  return Promise.all([closed(r), closed(b)]).then(function () { out("got", got, gotB); });
});

scenario("async iterator", function () {
  var r = new Readable({ objectMode: true, read: function () {} });
  r.push(1); r.push(2); r.push(3); r.push(null);
  var seen = [];
  return (async function () {
    for await (var x of r) seen.push(x);
    out("seen", seen, r.destroyed);
    var r2 = Readable.from([1, 2, 3, 4]);
    for await (var y of r2) { out("y", y); if (y === 2) break; }
    out("after break", r2.destroyed);
    var r3 = new Readable({ read: function () { this.destroy(new Error("iter fail")); } });
    try { for await (var z of r3) out("z", z); } catch (e) { out("caught", e); }
    var it = Readable.from(["q"]).iterator({ destroyOnReturn: false });
    out("iterator next", await it.next());
    out("iterator done", await it.next());
  })();
});

scenario("pipeline callback", function () {
  var out1 = [];
  return new Promise(function (resolve) {
    stream.pipeline(
      Readable.from(["a", "b", "c"]),
      new Transform({ transform: function (c, e, cb) { cb(null, String(c) + String(c)); } }),
      new Writable({ write: function (c, e, cb) { out1.push(String(c)); cb(); } }),
      function (err) { out("pipeline done", err, out1); resolve(); }
    );
  });
});

scenario("pipeline error and generators", function () {
  return new Promise(function (resolve) {
    var w = new Writable({ write: function (c, e, cb) { cb(); }, destroy: function (e, cb) { out("w destroyed", e); cb(e); } });
    stream.pipeline(
      async function* () { yield "a"; throw new Error("gen fail"); },
      async function* (source) { for await (var c of source) yield String(c) + "!"; },
      w,
      function (err) { out("pipeline err", err); resolve(); }
    );
  });
});

scenario("pipeline promises", async function () {
  var seen = [];
  await promises.pipeline(
    Readable.from(["x", "y"]),
    async function* (src) { for await (var c of src) yield String(c).toUpperCase(); },
    async function (src) { for await (var c of src) seen.push(c); }
  );
  out("seen", seen);
  try {
    await promises.pipeline(Readable.from(["x"]), new Transform({ transform: function (c, e, cb) { cb(new Error("t fail")); } }), new PassThrough());
  } catch (e) { out("rejected", e); }
  attempt("pipeline too few", function () { return stream.pipeline(new PassThrough()); });
  var ac = new AbortController();
  var p = promises.pipeline(new Readable({ read: function () {} }), new PassThrough(), { signal: ac.signal });
  ac.abort();
  try { await p; } catch (e) { out("aborted", e.name, e.code); }
});

scenario("finished", function () {
  var r = new Readable({ read: function () {} });
  r.push("a"); r.push(null);
  return new Promise(function (resolve) {
    var cleanup = stream.finished(r, function (err) { out("finished r", err); resolve(); });
    out("cleanup", typeof cleanup);
    r.resume();
  }).then(function () {
    var w = new Writable({ write: function (c, e, cb) { cb(); } });
    return new Promise(function (resolve) {
      stream.finished(w, { readable: false }, function (err) { out("finished w", err); resolve(); });
      w.destroy();
    });
  }).then(function () {
    var rr = Readable.from([1]);
    var done = promises.finished(rr).then(function () { out("promises.finished resolved"); });
    rr.resume();
    return done;
  }).then(function () {
    var d = new Duplex({ read: function () {}, write: function (c, e, cb) { cb(); } });
    return new Promise(function (resolve) {
      stream.finished(d, function (err) { out("finished duplex", err); resolve(); });
      d.destroy(new Error("dd"));
    });
  });
});

scenario("compose", function () {
  var c = stream.compose(
    new Transform({ transform: function (ch, e, cb) { cb(null, String(ch) + "1"); } }),
    async function* (src) { for await (var x of src) yield String(x) + "2"; }
  );
  var got = [];
  c.on("data", function (d) { got.push(String(d)); });
  c.end("a");
  out("compose is duplex", c instanceof Duplex);
  return closed(c).then(function () { out("got", got); });
});

scenario("operators", async function () {
  async function collect(r) { return r.toArray(); }
  out("map", await collect(Readable.from([1, 2, 3]).map(function (x) { return x * 2; })));
  out("filter", await collect(Readable.from([1, 2, 3, 4]).filter(function (x) { return x % 2 === 0; })));
  out("take", await collect(Readable.from([1, 2, 3, 4]).take(2)));
  out("drop", await collect(Readable.from([1, 2, 3, 4]).drop(2)));
  out("flatMap", await collect(Readable.from([1, 2]).flatMap(function (x) { return [x, x]; })));
  out("reduce", await Readable.from([1, 2, 3]).reduce(function (a, b) { return a + b; }, 10));
  out("some", await Readable.from([1, 2, 3]).some(function (x) { return x > 2; }));
  out("every", await Readable.from([1, 2, 3]).every(function (x) { return x > 2; }));
  out("find", await Readable.from([1, 2, 3]).find(function (x) { return x > 1; }));
  var seen = [];
  await Readable.from([1, 2]).forEach(function (x) { seen.push(x); });
  out("forEach", seen);
  out("asIndexedPairs", typeof Readable.prototype.asIndexedPairs);
  out("names", Object.getOwnPropertyNames(Readable.prototype).filter(function (n) { return ["map", "filter", "take", "drop", "flatMap", "toArray", "reduce", "forEach", "some", "every", "find", "compose"].indexOf(n) >= 0; }).sort());
  try { await Readable.from([1]).map(5).toArray(); } catch (e) { out("map bad fn", e); }
  try { await Readable.from([1]).take(-1).toArray(); } catch (e) { out("take bad", e); }
  attempt("new map", function () { return new Readable.prototype.map(function () {}); });
});

scenario("duplex", function () {
  var d = new Duplex({
    allowHalfOpen: false,
    readableObjectMode: true,
    writableObjectMode: false,
    read: function () {},
    write: function (c, e, cb) { out("d write", String(c)); cb(); },
  });
  out("modes", d.readableObjectMode, d.writableObjectMode, d.allowHalfOpen);
  watch("d", d, ["finish", "end", "close"]);
  d.push({ k: 1 });
  d.push(null);
  d.resume();
  d.end("w");
  return closed(d);
});

scenario("duplexPair and Duplex.from", async function () {
  var pair = stream.duplexPair();
  out("pair", pair.length, pair[0] instanceof Duplex);
  pair[0].write("ping");
  pair[0].end();
  var got = [];
  for await (var c of pair[1]) got.push(String(c));
  out("got", got);
  var d = Duplex.from(async function* (src) { for await (var x of src) yield String(x).toUpperCase(); });
  d.end("up");
  var r = [];
  for await (var y of d) r.push(String(y));
  out("Duplex.from", r);
});

scenario("errors and validation", function () {
  attempt("hwm string", function () { return new Readable({ highWaterMark: "x" }); });
  attempt("hwm negative", function () { return new Writable({ highWaterMark: -1 }); });
  try { new Readable().pipe(5); } catch (e) { out("pipe non-writable throws", e.name); }
  attempt("transform no impl", function () { var t = new Transform(); t.write("a"); return "queued"; });
  attempt("read not impl", function () { var r = new Readable(); r.on("error", function (e) { out("read err", e); }); r.read(); return "done"; });
  attempt("push after eof", function () { var r = new Readable({ read: function () {} }); r.on("error", function (e) { out("eof err", e); }); r.push(null); r.push("x"); return "pushed"; });
  attempt("write after end", function () { var w = new Writable({ write: function (c, e, cb) { cb(); } }); w.on("error", function (e) { out("wae", e); }); w.end(); w.write("x"); return "wrote"; });
  attempt("cb twice", function () { var w = new Writable({ write: function (c, e, cb) { cb(); cb(); } }); w.on("error", function (e) { out("cb twice", e); }); w.write("x"); return "wrote"; });
  attempt("addAbortSignal bad", function () { return stream.addAbortSignal({}, new PassThrough()); });
  attempt("compose none", function () { return stream.compose(); });
  return immediate();
});

scenario("module shape", function () {
  out("keys", Object.keys(stream));
  out("Stream", stream.Stream === stream, typeof stream.Readable, typeof stream.promises.pipeline);
  out("is*", stream.isReadable(new PassThrough()), stream.isErrored(new PassThrough()), stream.isDestroyed(new PassThrough()), stream.isDisturbed(new PassThrough()));
  out("hwm", stream.getDefaultHighWaterMark(false), stream.getDefaultHighWaterMark(true));
  stream.setDefaultHighWaterMark(false, 1024);
  out("hwm set", stream.getDefaultHighWaterMark(false), new Readable().readableHighWaterMark);
  stream.setDefaultHighWaterMark(false, 65536);
  out("promisify", typeof require("util").promisify(stream.pipeline), require("util").promisify(stream.finished) === promises.finished);
  out("proto", Object.getPrototypeOf(Readable) === stream.Stream, Object.getPrototypeOf(Duplex.prototype) === Readable.prototype);
  out("Writable hasInstance", Writable[Symbol.hasInstance](new Duplex()), new Writable() instanceof Duplex);
  out("states", Object.keys(new Readable()._readableState).length > 0, typeof new Writable()._writableState.getBuffer);
  return immediate();
});

scenario("legacy Stream pipe and ordering", function () {
  var legacy = new stream.Stream();
  var got = [];
  var w = new Writable({ write: function (c, e, cb) { got.push(String(c)); cb(); } });
  legacy.pipe(w);
  legacy.emit("data", "a");
  legacy.emit("data", "b");
  legacy.emit("end");
  tick("after legacy");
  micro("after legacy");
  return closed(w).then(function () { out("got", got); });
});
