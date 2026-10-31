// Deterministic differential corpus for the embedded web streams (#1360).
// Prints one line per observation plus a final digest line; the output must be
// byte-identical on Node and on jsse --node (scripts/run-node-webstreams-oracle.sh)
// and the digest is pinned in corpus.digest (tests/node_webstreams.rs).
// Scenarios log the order of chunks, promise settlements, microtasks and
// callbacks, so they pin the algorithm order of the Streams standard, not only
// the final values. Part A: ReadableStream and its readers.
var util = require("util");
var webstreams = require("stream/web");
var nodeStream = require("stream");

var lines = [];
function show(v) {
  if (typeof v === "string") return v;
  if (typeof v === "symbol") return String(v);
  if (typeof v === "function") return "[fn " + v.name + "]";
  if (v instanceof Error) return v.name + "|" + v.code + "|" + v.message;
  if (v instanceof Uint8Array) return "<" + Array.prototype.join.call(v, ",") + ">";
  if (v instanceof ArrayBuffer) return "ab" + v.byteLength;
  try {
    return JSON.stringify(v, function (k, x) {
      return typeof x === "symbol" ? String(x) : x;
    });
  } catch (e) {
    return String(v);
  }
}
function out() {
  lines.push(Array.prototype.map.call(arguments, show).join(" "));
}
function attempt(label, fn) {
  try { out(label, "->", fn()); } catch (e) { out(label, "throws", e); }
}
function settle(label, p) {
  return Promise.resolve(p).then(
    function (v) { out(label, "fulfilled", v); },
    function (e) { out(label, "rejected", e); }
  );
}
function micro(label) { Promise.resolve().then(function () { out("micro", label); }); }
function tick(label) { process.nextTick(function () { out("tick", label); }); }
function immediate() { return new Promise(function (resolve) { setImmediate(resolve); }); }
function inspect(v) { return util.inspect(v, { breakLength: Infinity }); }

var scenarios = [];
function scenario(name, fn) { scenarios.push([name, fn]); }

var ReadableStream = webstreams.ReadableStream;
var WritableStream = webstreams.WritableStream;
var TransformStream = webstreams.TransformStream;

scenario("module shape", function () {
  out(Object.keys(webstreams).sort().join(","));
  out(webstreams.ReadableStream === globalThis.ReadableStream, webstreams.TransformStream === globalThis.TransformStream);
  out(require("node:stream/web") === webstreams);
  out(Object.prototype.toString.call(new ReadableStream()), Object.prototype.toString.call(new WritableStream()));
  out(Object.getOwnPropertyNames(ReadableStream.prototype).sort().join(","));
  out(Object.getOwnPropertyNames(WritableStream.prototype).sort().join(","));
  out(Object.getOwnPropertyNames(TransformStream.prototype).sort().join(","));
  out(ReadableStream.length, WritableStream.length, TransformStream.length);
  out(typeof ReadableStream.from, typeof ReadableStream.prototype[Symbol.asyncIterator]);
  ["ReadableStream", "ReadableStreamDefaultReader", "ReadableStreamBYOBReader", "ReadableStreamBYOBRequest",
    "ReadableByteStreamController", "ReadableStreamDefaultController", "WritableStream", "WritableStreamDefaultWriter",
    "WritableStreamDefaultController", "TransformStream", "TransformStreamDefaultController", "ByteLengthQueuingStrategy",
    "CountQueuingStrategy", "TextEncoderStream", "TextDecoderStream", "CompressionStream", "DecompressionStream"
  ].forEach(function (name) {
    var touched = typeof globalThis[name];
    var d = Object.getOwnPropertyDescriptor(globalThis, name);
    out(name, touched, typeof d.value, d.writable, d.enumerable, d.configurable);
  });
});

scenario("inspect", function () {
  var rs = new ReadableStream();
  out(inspect(rs));
  out(inspect(new WritableStream()));
  out(inspect(new TransformStream()));
  out(inspect(rs.getReader()));
  out(inspect(new CountQueuingStrategy({ highWaterMark: 3 })));
  out(inspect(new ByteLengthQueuingStrategy({ highWaterMark: 4 })));
  out(inspect(new TextEncoderStream()));
  out(inspect(new TextDecoderStream("utf-16le", { fatal: true })));
  var closedStream = new ReadableStream({ start: function (c) { c.close(); } });
  out(inspect(closedStream));
  var erroredStream = new ReadableStream({ start: function (c) { c.error(new Error("boom")); } });
  out(inspect(erroredStream));
  out(inspect(new ReadableStream({ type: "bytes" })));
  var depth = inspect({ rs: rs });
  out(depth);
});

scenario("constructor validation", function () {
  attempt("source number", function () { return new ReadableStream(1); });
  attempt("source null", function () { return new ReadableStream(null) instanceof ReadableStream; });
  attempt("type bad", function () { return new ReadableStream({ type: "x" }); });
  attempt("hwm negative", function () { return new ReadableStream({}, { highWaterMark: -1 }); });
  attempt("hwm NaN", function () { return new ReadableStream({}, { highWaterMark: NaN }); });
  attempt("size not fn", function () { return new ReadableStream({}, { size: 1 }); });
  attempt("bytes with size", function () { return new ReadableStream({ type: "bytes" }, { size: function () { return 1; } }); });
  attempt("start not fn", function () { return new ReadableStream({ start: 1 }); });
  attempt("autoAllocate zero", function () { return new ReadableStream({ type: "bytes", autoAllocateChunkSize: 0 }); });
  attempt("call without new", function () { return ReadableStream(); });
  attempt("controller ctor", function () { return new ReadableStreamDefaultController(); });
  attempt("reader locked", function () {
    var rs = new ReadableStream();
    rs.getReader();
    return rs.getReader();
  });
  attempt("reader mode", function () { return new ReadableStream().getReader({ mode: "x" }); });
  attempt("byob on default", function () { return new ReadableStream().getReader({ mode: "byob" }); });
  attempt("illegal this", function () { return ReadableStream.prototype.getReader.call({}); });
  attempt("locked getter", function () { return Object.getOwnPropertyDescriptor(ReadableStream.prototype, "locked").get.call(1); });
});

scenario("default reader order", function () {
  var log = [];
  var rs = new ReadableStream({
    start: function (c) { out("start"); c.enqueue("a"); c.enqueue("b"); },
    pull: function (c) { out("pull", c.desiredSize); if (log.length === 0) { log.push(1); c.enqueue("c"); c.close(); } },
    cancel: function (r) { out("cancel", r); }
  }, { highWaterMark: 2 });
  var reader = rs.getReader();
  micro("1");
  var chain = reader.read().then(function (r) { out("read1", r); return reader.read(); })
    .then(function (r) { out("read2", r); return reader.read(); })
    .then(function (r) { out("read3", r); return reader.read(); })
    .then(function (r) { out("read4", r); return reader.closed; })
    .then(function (v) { out("closed", v); });
  micro("2");
  return chain;
});

scenario("backpressure and desiredSize", function () {
  var ctrl;
  var rs = new ReadableStream({ start: function (c) { ctrl = c; } }, { highWaterMark: 3 });
  out(ctrl.desiredSize);
  ctrl.enqueue(1);
  ctrl.enqueue(2);
  out(ctrl.desiredSize);
  ctrl.enqueue(3);
  ctrl.enqueue(4);
  out(ctrl.desiredSize);
  var reader = rs.getReader();
  return reader.read().then(function (r) {
    out(r, ctrl.desiredSize);
    ctrl.close();
    attempt("enqueue after close", function () { ctrl.enqueue(5); });
    attempt("close twice", function () { ctrl.close(); });
    out(ctrl.desiredSize);
  });
});

scenario("size function", function () {
  var sizes = [];
  var ctrl;
  new ReadableStream({ start: function (c) { ctrl = c; } }, {
    highWaterMark: 10,
    size: function (chunk) { sizes.push(chunk); return chunk.length; }
  });
  ctrl.enqueue("abc");
  ctrl.enqueue("de");
  out(sizes, ctrl.desiredSize);
  attempt("size throws", function () {
    var s = new ReadableStream({ start: function (c) { ctrl = c; } }, { size: function () { throw new RangeError("size"); } });
    ctrl.enqueue("x");
  });
  attempt("size negative", function () {
    new ReadableStream({ start: function (c) { ctrl = c; } }, { size: function () { return -1; } });
    ctrl.enqueue("x");
  });
});

scenario("cancel", function () {
  var rs = new ReadableStream({ cancel: function (reason) { out("cancel called", reason); return Promise.resolve("ignored"); } });
  return settle("cancel", rs.cancel("why")).then(function () {
    return settle("cancel again", rs.cancel("again"));
  }).then(function () {
    out(inspect(rs));
    var reader = new ReadableStream().getReader();
    return settle("reader cancel", reader.cancel("r"));
  }).then(function () {
    var locked = new ReadableStream();
    locked.getReader();
    return settle("cancel locked", locked.cancel());
  });
});

scenario("error propagation", function () {
  var rs = new ReadableStream({
    start: function (c) { c.enqueue("x"); },
    pull: function () { throw new Error("pull failed"); }
  });
  var reader = rs.getReader();
  return settle("read after pull throws", reader.read()).then(function () {
    return settle("read again", reader.read());
  }).then(function () {
    return settle("closed", reader.closed);
  }).then(function () {
    attempt("start throws", function () {
      return new ReadableStream({ start: function () { throw new Error("start failed"); } });
    });
  });
});

scenario("releaseLock", function () {
  var rs = new ReadableStream({ start: function (c) { c.enqueue(1); } });
  var reader = rs.getReader();
  var pending = reader.read();
  var second = reader.read();
  reader.releaseLock();
  out(rs.locked);
  return settle("read1", pending).then(function () {
    return settle("read2", second);
  }).then(function () {
    return settle("closed after release", reader.closed);
  }).then(function () {
    return settle("read after release", reader.read());
  });
});

scenario("tee", function () {
  var rs = new ReadableStream({
    start: function (c) { c.enqueue("a"); c.enqueue("b"); c.close(); }
  });
  var branches = rs.tee();
  out(branches.length, rs.locked, branches[0] !== branches[1]);
  function drain(label, branch) {
    var reader = branch.getReader();
    var items = [];
    return (function loop() {
      return reader.read().then(function (r) {
        if (r.done) { out(label, items.join(",")); return; }
        items.push(r.value);
        return loop();
      });
    })();
  }
  return Promise.all([drain("branch0", branches[0]), drain("branch1", branches[1])]);
});

scenario("tee cancel", function () {
  var reasons = [];
  var rs = new ReadableStream({ cancel: function (reason) { reasons.push(reason); out("source cancel", reason); } });
  var b = rs.tee();
  var first = settle("cancel one", b[0].cancel("one"));
  return immediate().then(function () {
    out("after first", reasons.length);
    return Promise.all([first, settle("cancel two", b[1].cancel("two"))]);
  }).then(function () {
    out("after second", reasons.length);
  });
});

scenario("async iterator", function () {
  var rs = new ReadableStream({
    start: function (c) { c.enqueue(1); c.enqueue(2); c.enqueue(3); c.close(); }
  });
  var seen = [];
  return (async function () {
    for await (var v of rs) { seen.push(v); }
    out("seen", seen, rs.locked);
    var rs2 = new ReadableStream({
      start: function (c) { c.enqueue(1); c.enqueue(2); },
      cancel: function (r) { out("iter cancel", r); }
    });
    for await (var w of rs2) { out("got", w); break; }
    out("after break", rs2.locked);
    var rs3 = new ReadableStream({ start: function (c) { c.enqueue(1); }, cancel: function () { out("keep"); } });
    for await (var z of rs3.values({ preventCancel: true })) { out("prevent", z); break; }
    out("prevent locked", rs3.locked);
    var it = new ReadableStream().values();
    out(Object.prototype.toString.call(it), typeof it.next, typeof it.return);
  })();
});

scenario("ReadableStream.from", function () {
  return (async function () {
    var items = [];
    for await (var v of ReadableStream.from([1, 2, 3])) items.push(v);
    out("array", items);
    items = [];
    async function* gen() { yield "x"; yield "y"; }
    for await (var w of ReadableStream.from(gen())) items.push(w);
    out("async gen", items);
    attempt("from number", function () { return ReadableStream.from(1); });
    var thrown = ReadableStream.from({
      [Symbol.iterator]: function () {
        return { next: function () { throw new Error("iter next"); } };
      }
    });
    await settle("from throwing", thrown.getReader().read());
    var returned = false;
    var source = ReadableStream.from({
      [Symbol.iterator]: function () {
        return { next: function () { return { value: 1, done: false }; }, return: function () { returned = true; return {}; } };
      }
    });
    var r = source.getReader();
    await r.read();
    await r.cancel("c");
    out("iterator return called", returned);
  })();
});

scenario("byte stream default reader", function () {
  var rs = new ReadableStream({
    type: "bytes",
    start: function (c) { c.enqueue(new Uint8Array([1, 2, 3])); c.enqueue(new Uint8Array([4])); c.close(); }
  });
  var reader = rs.getReader();
  return reader.read().then(function (r) {
    out(r.done, r.value, Object.getPrototypeOf(r.value) === Uint8Array.prototype);
    return reader.read();
  }).then(function (r) {
    out(r.value);
    return reader.read();
  }).then(function (r) { out(r); });
});

scenario("byob reader", function () {
  var pulls = 0;
  var rs = new ReadableStream({
    type: "bytes",
    pull: function (c) {
      pulls++;
      var req = c.byobRequest;
      out("pull", pulls, req === null ? "null" : req.view.byteLength);
      if (req) {
        req.view[0] = 10 * pulls;
        req.respond(1);
      } else {
        c.enqueue(new Uint8Array([7, 8]));
      }
      if (pulls === 3) c.close();
    }
  });
  var reader = rs.getReader({ mode: "byob" });
  var buffer = new ArrayBuffer(4);
  return reader.read(new Uint8Array(buffer, 0, 2)).then(function (r) {
    out("r1", r.done, r.value, r.value.byteOffset, r.value.buffer.byteLength, buffer.byteLength);
    return reader.read(new Uint8Array(4), { min: 1 });
  }).then(function (r) {
    out("r2", r.done, r.value);
    return reader.read(new Uint8Array(2));
  }).then(function (r) {
    out("r3", r.done, r.value);
    return settle("empty view", reader.read(new Uint8Array(0)));
  }).then(function () {
    return settle("not a view", reader.read({}));
  }).then(function () {
    return settle("min too big", reader.read(new Uint8Array(1), { min: 2 }));
  });
});

scenario("byob request validation", function () {
  var rs = new ReadableStream({
    type: "bytes",
    pull: function (c) {
      var req = c.byobRequest;
      attempt("respond too big", function () { req.respond(100); });
      attempt("respondWithNewView bad", function () { req.respondWithNewView(new Uint8Array(100)); });
      req.respond(2);
    }
  });
  return rs.getReader({ mode: "byob" }).read(new Uint8Array(2)).then(function (r) { out(r.value); });
});

scenario("autoAllocateChunkSize", function () {
  var rs = new ReadableStream({
    type: "bytes",
    autoAllocateChunkSize: 3,
    pull: function (c) {
      out("pull byobRequest", c.byobRequest ? c.byobRequest.view.byteLength : null);
      c.byobRequest.view.set([1, 2, 3]);
      c.byobRequest.respond(3);
      c.close();
    }
  });
  return rs.getReader().read().then(function (r) { out(r.value, r.done); });
});

scenario("tee byte stream", function () {
  var rs = new ReadableStream({
    type: "bytes",
    start: function (c) { c.enqueue(new Uint8Array([1, 2])); c.close(); }
  });
  var b = rs.tee();
  return Promise.all([b[0].getReader().read(), b[1].getReader().read()]).then(function (rs) {
    out(rs[0].value, rs[1].value, rs[0].value !== rs[1].value);
  });
});

scenario("queueMicrotask and nextTick order around read", function () {
  var rs = new ReadableStream({ start: function (c) { c.enqueue("v"); } });
  var reader = rs.getReader();
  tick("t1");
  micro("m1");
  var p = reader.read().then(function (r) { out("read", r); });
  micro("m2");
  tick("t2");
  return p;
});
