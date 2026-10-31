// Part B of the web streams corpus (see corpus-a.js): WritableStream,
// TransformStream, piping, queuing strategies, the text and compression
// streams, and the Node stream interop (toWeb/fromWeb).

scenario("writable order", function () {
  var sink = {
    start: function () { out("start"); },
    write: function (chunk, c) { out("write", chunk); return new Promise(function (r) { setImmediate(r); }); },
    close: function () { out("close"); },
    abort: function (reason) { out("abort", reason); }
  };
  var ws = new WritableStream(sink, { highWaterMark: 2 });
  var writer = ws.getWriter();
  out(writer.desiredSize);
  var ready0 = writer.ready;
  var w1 = writer.write("a");
  out(writer.desiredSize);
  var w2 = writer.write("b");
  out(writer.desiredSize);
  var w3 = writer.write("c");
  out(writer.desiredSize);
  return settle("w1", w1).then(function () {
    out(writer.desiredSize);
    return settle("w2", w2);
  }).then(function () {
    return settle("w3", w3);
  }).then(function () {
    return writer.close();
  }).then(function () {
    return settle("closed", writer.closed);
  }).then(function () {
    out(inspect(ws));
    return settle("ready0", ready0);
  });
});

scenario("writable abort and error", function () {
  var ws = new WritableStream({
    write: function (chunk) { if (chunk === "bad") throw new Error("sink write"); },
    abort: function (reason) { out("abort", reason); }
  });
  var writer = ws.getWriter();
  return settle("good", writer.write("ok")).then(function () {
    return settle("bad", writer.write("bad"));
  }).then(function () {
    return settle("closed", writer.closed);
  }).then(function () {
    return settle("after error", writer.write("x"));
  }).then(function () {
    return settle("abort errored", writer.abort("late"));
  }).then(function () {
    var ws2 = new WritableStream({ abort: function (r) { out("abort2", r); } });
    return settle("abort", ws2.abort("because")).then(function () {
      return settle("write after abort", ws2.getWriter().write(1));
    });
  });
});

scenario("writable validation", function () {
  attempt("sink number", function () { return new WritableStream(1); });
  attempt("sink type", function () { return new WritableStream({ type: "bytes" }); });
  attempt("hwm", function () { return new WritableStream({}, { highWaterMark: -2 }); });
  attempt("double writer", function () { var ws = new WritableStream(); ws.getWriter(); return ws.getWriter(); });
  attempt("controller ctor", function () { return new WritableStreamDefaultController(); });
  attempt("writer of locked", function () { return new WritableStreamDefaultWriter(1); });
  var ws = new WritableStream();
  var writer = ws.getWriter();
  writer.releaseLock();
  return settle("write released", writer.write("x")).then(function () {
    return settle("closed released", writer.closed);
  }).then(function () {
    return settle("close locked", (function () { ws.getWriter(); return ws.close(); })());
  });
});

scenario("writable controller signal", function () {
  var ctrl;
  var ws = new WritableStream({ start: function (c) { ctrl = c; } });
  out(ctrl.signal.aborted);
  var seen = [];
  ctrl.signal.addEventListener("abort", function () { seen.push("abort-event"); });
  return ws.abort("stop").then(function () {
    out(ctrl.signal.aborted, ctrl.signal.reason, seen);
    attempt("error after abort", function () { return ctrl.error(new Error("ignored")); });
  });
});

scenario("transform identity", function () {
  var ts = new TransformStream();
  var writer = ts.writable.getWriter();
  var reader = ts.readable.getReader();
  var results = [];
  var read = reader.read().then(function (r) { results.push(r); return reader.read(); })
    .then(function (r) { results.push(r); return reader.read(); })
    .then(function (r) { results.push(r); });
  writer.write("a");
  writer.write("b");
  writer.close();
  return read.then(function () { out(results); });
});

scenario("transform order", function () {
  var ts = new TransformStream({
    start: function (c) { out("start", c.desiredSize); },
    transform: function (chunk, c) {
      out("transform", chunk, c.desiredSize);
      c.enqueue(chunk + "1");
      c.enqueue(chunk + "2");
    },
    flush: function (c) { out("flush"); c.enqueue("end"); }
  }, { highWaterMark: 1 }, { highWaterMark: 0 });
  var writer = ts.writable.getWriter();
  var reader = ts.readable.getReader();
  var got = [];
  var done = (function loop() {
    return reader.read().then(function (r) {
      if (r.done) return;
      got.push(r.value);
      return loop();
    });
  })();
  writer.write("x");
  writer.write("y");
  writer.close();
  return done.then(function () { out(got); });
});

scenario("transform errors", function () {
  var ts = new TransformStream({
    transform: function (chunk, c) { if (chunk === "bad") throw new Error("transform"); c.enqueue(chunk); },
    flush: function () { throw new Error("flush"); }
  });
  var writer = ts.writable.getWriter();
  var reader = ts.readable.getReader();
  var p1 = settle("write ok", writer.write("ok"));
  var p2 = settle("read ok", reader.read());
  return Promise.all([p1, p2]).then(function () {
    var pending = settle("read bad", reader.read());
    return settle("write bad", writer.write("bad")).then(function () { return pending; });
  }).then(function () {
    return settle("read after bad", reader.read());
  }).then(function () {
    attempt("transformer type", function () { return new TransformStream({ readableType: "bytes" }); });
    attempt("transformer writableType", function () { return new TransformStream({ writableType: "x" }); });
    var ts2 = new TransformStream({ start: function (c) { c.terminate(); } });
    return settle("terminated read", ts2.readable.getReader().read());
  });
});

scenario("pipeTo", function () {
  var chunks = [];
  var rs = new ReadableStream({
    start: function (c) { c.enqueue(1); c.enqueue(2); c.close(); }
  });
  var ws = new WritableStream({
    write: function (c) { chunks.push(c); },
    close: function () { out("sink closed"); }
  });
  return rs.pipeTo(ws).then(function () {
    out("piped", chunks, rs.locked, ws.locked);
    var rs2 = new ReadableStream({ start: function (c) { c.error(new Error("src")); } });
    return settle("error source", rs2.pipeTo(new WritableStream({ abort: function (r) { out("dest abort", r); } })));
  }).then(function () {
    var rs3 = new ReadableStream({ start: function (c) { c.enqueue(1); }, cancel: function (r) { out("src cancel", r); } });
    var ws3 = new WritableStream({ write: function () { throw new Error("dest"); } });
    return settle("error dest", rs3.pipeTo(ws3));
  }).then(function () {
    var ac = new AbortController();
    var rs4 = new ReadableStream({ cancel: function (r) { out("abort cancel", r); } });
    var ws4 = new WritableStream({ abort: function (r) { out("abort dest", r); } });
    var p = rs4.pipeTo(ws4, { signal: ac.signal });
    ac.abort("sig");
    return settle("signal", p);
  }).then(function () {
    var rs5 = new ReadableStream({ start: function (c) { c.close(); } });
    var ws5 = new WritableStream({ close: function () { out("kept open"); } });
    return rs5.pipeTo(ws5, { preventClose: true }).then(function () { out("ws5 locked", ws5.locked); });
  }).then(function () {
    return settle("locked pipe", (function () {
      var r = new ReadableStream();
      r.getReader();
      return r.pipeTo(new WritableStream());
    })());
  });
});

scenario("pipeThrough", function () {
  var upper = new TransformStream({ transform: function (c, ctrl) { ctrl.enqueue(String(c).toUpperCase()); } });
  var rs = new ReadableStream({ start: function (c) { c.enqueue("a"); c.enqueue("b"); c.close(); } });
  var out2 = rs.pipeThrough(upper);
  out(out2 === upper.readable, rs.locked, upper.writable.locked);
  var items = [];
  return (async function () {
    for await (var v of out2) items.push(v);
    out("through", items);
    attempt("bad pair", function () { return new ReadableStream().pipeThrough({}); });
    attempt("bad readable", function () { return new ReadableStream().pipeThrough({ readable: 1, writable: new WritableStream() }); });
  })();
});

scenario("queuing strategies", function () {
  var count = new CountQueuingStrategy({ highWaterMark: 5 });
  var bytes = new ByteLengthQueuingStrategy({ highWaterMark: 7 });
  out(count.highWaterMark, count.size("anything"), bytes.highWaterMark, bytes.size(new Uint8Array(3)), bytes.size({ byteLength: 9 }));
  out(count.size.name, bytes.size.name, count.size.length, bytes.size.length);
  attempt("missing hwm", function () { return new CountQueuingStrategy({}); });
  attempt("no init", function () { return new CountQueuingStrategy(); });
  attempt("hwm string", function () { return new CountQueuingStrategy({ highWaterMark: "3" }).highWaterMark; });
  attempt("bad this", function () { return Object.getOwnPropertyDescriptor(CountQueuingStrategy.prototype, "highWaterMark").get.call({}); });
  attempt("call", function () { return CountQueuingStrategy({ highWaterMark: 1 }); });
});

scenario("text encoder and decoder streams", function () {
  var enc = new TextEncoderStream();
  var dec = new TextDecoderStream();
  out(enc.encoding, dec.encoding, dec.fatal, dec.ignoreBOM);
  var piped = new ReadableStream({
    start: function (c) { c.enqueue("hé"); c.enqueue("llo \ud83d"); c.enqueue("\ude00"); c.close(); }
  }).pipeThrough(enc);
  var collected = [];
  return (async function () {
    for await (var chunk of piped) collected.push(Array.prototype.slice.call(chunk));
    out("encoded", collected);
    var d = new ReadableStream({
      start: function (c) { c.enqueue(new Uint8Array([0xc3])); c.enqueue(new Uint8Array([0xa9, 0x41])); c.close(); }
    }).pipeThrough(new TextDecoderStream());
    var text = "";
    for await (var s of d) text += s;
    out("decoded", text);
    var w = enc.writable.getWriter();
    await settle("bad chunk", new ReadableStream({ start: function (c) { c.enqueue(undefined); c.close(); } })
      .pipeThrough(new TextDecoderStream()).getReader().read());
  })();
});

scenario("compression streams", function () {
  return (async function () {
    attempt("bad format", function () { return new CompressionStream("zip"); });
    var source = new ReadableStream({
      start: function (c) { c.enqueue(new TextEncoder().encode("hello hello hello hello")); c.close(); }
    });
    var compressed = source.pipeThrough(new CompressionStream("gzip"));
    var parts = [];
    for await (var chunk of compressed) parts.push(chunk);
    var total = parts.reduce(function (n, p) { return n + p.length; }, 0);
    out("gzip magic", parts[0][0], parts[0][1], total > 10);
    var back = new ReadableStream({
      start: function (c) { parts.forEach(function (p) { c.enqueue(p); }); c.close(); }
    }).pipeThrough(new DecompressionStream("gzip"));
    var text = "";
    var dec = new TextDecoder();
    for await (var piece of back) text += dec.decode(piece, { stream: true });
    out("round trip", text);
    var broken = new ReadableStream({ start: function (c) { c.enqueue(new Uint8Array([1, 2, 3, 4])); c.close(); } })
      .pipeThrough(new DecompressionStream("deflate"));
    await settle("bad data", broken.getReader().read());
  })();
});

scenario("Readable.toWeb and fromWeb", function () {
  return (async function () {
    var web = nodeStream.Readable.toWeb(nodeStream.Readable.from(["a", "b", "c"]));
    out(web instanceof ReadableStream);
    var items = [];
    for await (var v of web) items.push(v);
    out("toWeb", items);
    var rs = new ReadableStream({ start: function (c) { c.enqueue("x"); c.enqueue("y"); c.close(); } });
    var node = nodeStream.Readable.fromWeb(rs, { objectMode: true });
    items = [];
    for await (var w of node) items.push(w);
    out("fromWeb", items);
    var rb = nodeStream.Readable.toWeb(new nodeStream.Readable({ read: function () { this.push(Buffer.from("bytes")); this.push(null); } }));
    var r = rb.getReader();
    var first = await r.read();
    out("buffer chunk", Buffer.isBuffer(first.value), first.value instanceof Uint8Array, String(first.value));
    attempt("toWeb non-readable", function () { return nodeStream.Readable.toWeb({}); });
    attempt("fromWeb non-stream", function () { return nodeStream.Readable.fromWeb({}); });
    var errored = nodeStream.Readable.toWeb(new nodeStream.Readable({ read: function () { this.destroy(new Error("node boom")); } }));
    await settle("errored", errored.getReader().read());
  })();
});

scenario("Writable.toWeb and fromWeb", function () {
  return (async function () {
    var got = [];
    var nodeWritable = new nodeStream.Writable({
      write: function (chunk, enc, cb) { got.push(String(chunk)); cb(); },
      final: function (cb) { out("node final"); cb(); }
    });
    var web = nodeStream.Writable.toWeb(nodeWritable);
    out(web instanceof WritableStream);
    var writer = web.getWriter();
    await writer.write("one");
    await writer.write("two");
    await writer.close();
    out("toWeb", got);
    var sunk = [];
    var ws = new WritableStream({
      write: function (c) { sunk.push(String(c)); },
      close: function () { out("web close"); }
    });
    var node = nodeStream.Writable.fromWeb(ws, { decodeStrings: false });
    node.write("p");
    node.end("q");
    await new Promise(function (resolve) { node.on("close", resolve); });
    out("fromWeb", sunk);
    attempt("fromWeb non-stream", function () { return nodeStream.Writable.fromWeb({}); });
  })();
});

scenario("Duplex.toWeb and fromWeb", function () {
  return (async function () {
    var d = new nodeStream.PassThrough();
    var pair = nodeStream.Duplex.toWeb(d);
    out(Object.keys(pair).join(","), pair.readable instanceof ReadableStream, pair.writable instanceof WritableStream);
    var writer = pair.writable.getWriter();
    var reader = pair.readable.getReader();
    await writer.write(Buffer.from("echo"));
    out("echo", String((await reader.read()).value));
    await writer.close();
    out("end", await reader.read());
    var ts = new TransformStream({ transform: function (c, ctrl) { ctrl.enqueue(String(c) + "!"); } });
    var duplex = nodeStream.Duplex.fromWeb(ts, { objectMode: true });
    duplex.write("hey");
    duplex.end();
    var items = [];
    for await (var v of duplex) items.push(v);
    out("fromWeb", items);
    attempt("fromWeb bad", function () { return nodeStream.Duplex.fromWeb({}); });
  })();
});

scenario("Node stream helpers over web streams", function () {
  return (async function () {
    var rs = new ReadableStream({ start: function (c) { c.enqueue("a"); c.enqueue("b"); c.close(); } });
    var items = [];
    await nodeStream.promises.pipeline(rs, async function* (source) { for await (var v of source) yield v + v; },
      new WritableStream({ write: function (c) { items.push(c); } }));
    out("pipeline", items);
    var d = nodeStream.Duplex.from(new ReadableStream({ start: function (c) { c.enqueue("z"); c.close(); } }));
    items = [];
    for await (var v of d) items.push(String(v));
    out("Duplex.from rs", items);
    var finishedLog = [];
    var ws = new WritableStream();
    var w = ws.getWriter();
    var done = nodeStream.promises.finished(ws);
    await w.close();
    await done;
    out("finished writable");
    var r2 = new ReadableStream({ start: function (c) { c.close(); } });
    await nodeStream.promises.finished(r2);
    out("finished readable");
    var iter = nodeStream.Readable.from(new ReadableStream({ start: function (c) { c.enqueue(1); c.close(); } }));
    items = [];
    for await (var x of iter) items.push(x);
    out("Readable.from rs", items);
  })();
});

scenario("Blob streams", function () {
  return (async function () {
    var blob = new (require("buffer").Blob)(["hello ", "blob"]);
    var chunks = [];
    for await (var c of blob.stream()) chunks.push(Buffer.from(c).toString());
    out("blob.stream", chunks.join(""));
    var text = "";
    for await (var t of blob.textStream()) text += t;
    out("blob.textStream", text);
    var consumers = require("stream/consumers");
    out("text", await consumers.text(blob.stream()));
    out("json", await consumers.json(new ReadableStream({ start: function (c) { c.enqueue('{"a":'); c.enqueue("1}"); c.close(); } }).pipeThrough(new TextEncoderStream())));
    out("buffer", String(await consumers.buffer(blob.stream())));
    out("arrayBuffer", (await consumers.arrayBuffer(blob.stream())).byteLength);
    out("blob", (await consumers.blob(blob.stream())).size);
  })();
});
