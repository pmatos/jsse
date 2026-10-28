// fs.watch, fs.watchFile and fs.promises.watch (issue #1235). Timing-free: an
// operation is followed by a sentinel directory, and the step ends when the
// sentinel's event arrives, because events from one inotify queue are ordered.
// Each step prints the normalised set of "type:name" events it produced.
const fs = require("fs");
const fsp = require("fs/promises");
const D = process.env.FS_ORACLE_DIR;
if (!D) throw new Error("FS_ORACLE_DIR is not set");
const norm = (s) => String(s).split(D).join("<D>");
const W = D + "/watched";
const watchdog = setTimeout(() => {
  console.log("TIMEOUT");
  process.exit(1);
}, 300000);
watchdog.unref();

let marks = 0;
function collect(path, options, quiet) {
  const events = [];
  let waiting = null;
  const watcher = fs.watch(path, options, (type, name) => {
    const text = Buffer.isBuffer(name) ? name.toString() : name;
    if (text.startsWith("mark-")) {
      if (waiting && waiting.mark === text) waiting.resolve();
      return;
    }
    if (!quiet) events.push(type + ":" + text);
  });
  return {
    watcher,
    events,
    settle(mark) {
      return new Promise((resolve) => { waiting = { mark, resolve }; });
    },
  };
}

async function step(label, probe, op, sentinel = probe) {
  probe.events.length = 0;
  sentinel.events.length = 0;
  op();
  const mark = "mark-" + ++marks;
  const settled = sentinel.settle(mark);
  fs.mkdirSync(W + "/" + mark);
  await settled;
  console.log(label.padEnd(18), [...new Set(probe.events)].sort().join(" ") || "(none)");
}

function fail(label, fn) {
  try { fn(); console.log(label, "no throw"); } catch (e) {
    console.log(label, e.name, e.code, e.errno, e.syscall, norm(e.path), norm(e.filename), norm(e.message));
  }
}

async function main() {
  fs.mkdirSync(W);
  fs.writeFileSync(W + "/a.txt", "one");
  fs.mkdirSync(W + "/sub");

  console.log("-- directory watch");
  const dir = collect(W);
  console.log(dir.watcher.constructor.name, typeof dir.watcher.ref, dir.watcher.ref() === dir.watcher, dir.watcher.unref() === dir.watcher);
  dir.watcher.ref();
  await step("create", dir, () => fs.writeFileSync(W + "/b.txt", "x"));
  await step("modify", dir, () => fs.appendFileSync(W + "/b.txt", "y"));
  await step("rename", dir, () => fs.renameSync(W + "/b.txt", W + "/c.txt"));
  await step("chmod", dir, () => fs.chmodSync(W + "/c.txt", 0o600));
  await step("delete", dir, () => fs.unlinkSync(W + "/c.txt"));
  await step("mkdir", dir, () => fs.mkdirSync(W + "/d"));
  await step("rmdir", dir, () => fs.rmdirSync(W + "/d"));
  await step("nested (none)", dir, () => fs.writeFileSync(W + "/sub/deep.txt", "x"));

  console.log("-- file watch");
  const file = collect(W + "/a.txt");
  await step("modify file", file, () => fs.appendFileSync(W + "/a.txt", "two"), dir);
  await step("chmod file", file, () => fs.chmodSync(W + "/a.txt", 0o640), dir);
  await step("delete file", file, () => fs.unlinkSync(W + "/a.txt"), dir);
  file.watcher.close();

  console.log("-- two watchers, one path");
  const first = collect(W);
  const second = collect(W);
  await step("both see", first, () => fs.writeFileSync(W + "/e.txt", "x"));
  console.log("second saw", [...new Set(second.events)].sort().join(" "));
  second.watcher.close();

  console.log("-- encoding buffer");
  const raw = [];
  const bufferWatcher = fs.watch(W, { encoding: "buffer" }, (type, name) => {
    if (!name.toString().startsWith("mark-")) raw.push(type + ":" + Buffer.isBuffer(name) + ":" + name.toString("hex"));
  });
  await step("buffer name", first, () => fs.writeFileSync(W + "/f.txt", "x"));
  console.log("buffer events", [...new Set(raw)].sort().join(" "));
  bufferWatcher.close();

  console.log("-- close");
  const closing = fs.watch(W);
  const log = [];
  closing.on("close", () => log.push("close"));
  closing.close();
  closing.close();
  log.push("closed-sync");
  console.log(closing._handle === null, closing.ref() === closing, closing.unref() === closing);
  await new Promise((resolve) => setImmediate(resolve));
  console.log(log.join(","));
  const aborter = new AbortController();
  const aborted = fs.watch(W, { signal: aborter.signal });
  const abortLog = new Promise((resolve) => aborted.on("close", resolve));
  aborter.abort();
  await abortLog;
  console.log("abort closed", aborted._handle === null);

  console.log("-- errors");
  fail("missing", () => fs.watch(W + "/missing"));
  fail("number", () => fs.watch(5));
  fail("encoding", () => fs.watch(W, { encoding: "nope" }));
  fail("recursive", () => fs.watch(W, { recursive: 1 }));
  fail("options", () => fs.watch(W, 1));
  fail("signal", () => fs.watch(W, { signal: 3 }));

  console.log("-- recursive");
  fs.mkdirSync(W + "/tree/one/two", { recursive: true });
  fs.writeFileSync(W + "/tree/one/two/leaf.txt", "x");
  const rec = collect(W + "/tree", { recursive: true });
  const rstep = async (label, expected, op) => {
    rec.events.length = 0;
    op();
    const mark = "mark-" + ++marks;
    const settled = rec.settle(mark);
    fs.mkdirSync(W + "/tree/" + mark);
    await settled;
    console.log(label.padEnd(18), rec.events.some((e) => e.split(":")[1] === expected));
  };
  await rstep("modify deep", "one/two/leaf.txt", () => fs.appendFileSync(W + "/tree/one/two/leaf.txt", "y"));
  await rstep("create deep", "one/two/new.txt", () => fs.writeFileSync(W + "/tree/one/two/new.txt", "x"));
  await rstep("create dir", "fresh", () => fs.mkdirSync(W + "/tree/fresh"));
  // Node adopts a new directory asynchronously, so probe until it has.
  for (let attempt = 0; ; attempt++) {
    rec.events.length = 0;
    fs.writeFileSync(W + "/tree/fresh/probe-" + attempt, "x");
    const mark = "mark-" + ++marks;
    const settled = rec.settle(mark);
    fs.mkdirSync(W + "/tree/" + mark);
    await settled;
    if (rec.events.some((e) => e.endsWith("fresh/probe-" + attempt))) break;
  }
  console.log("new dir adopted    true");
  await rstep("rename deep", "fresh/out.txt", () => fs.renameSync(W + "/tree/fresh/probe-0", W + "/tree/fresh/out.txt"));
  rec.watcher.close();

  console.log("-- watchFile");
  const target = W + "/polled.txt";
  const seen = [];
  let onchange = null;
  const listener = (cur, prev) => {
    seen.push(["cur", cur.size, cur.mtimeMs > 0, "prev", prev.size, prev.mtimeMs > 0].join(" "));
    if (onchange) onchange();
  };
  const changed = () => new Promise((resolve) => { onchange = resolve; });
  let next = changed();
  const poller = fs.watchFile(target, { interval: 10 }, listener);
  console.log(poller.constructor.name, poller === fs.watchFile(target, { interval: 10 }, () => {}));
  await next;
  next = changed(); fs.writeFileSync(target, "abc"); await next;
  next = changed(); fs.writeFileSync(target, "abcdef"); await next;
  next = changed(); fs.unlinkSync(target); await next;
  console.log(seen.join("\n"));
  fs.unwatchFile(target, listener);
  console.log("listeners left", poller.listenerCount("change"));
  fs.unwatchFile(target);
  console.log("after unwatch", poller.listenerCount("change"));
  fail("no listener", () => fs.watchFile(target));
  fail("bad interval", () => fs.watchFile(target, { interval: "x" }, () => {}));
  const big = [];
  let bigNext = null;
  fs.watchFile(W + "/big.txt", { interval: 10, bigint: true }, (cur) => { big.push(typeof cur.size + ":" + cur.size); if (bigNext) bigNext(); });
  await new Promise((resolve) => { bigNext = resolve; });
  bigNext = null;
  fs.unwatchFile(W + "/big.txt");
  console.log("bigint", big.join());

  console.log("-- promises.watch");
  const events = [];
  const iterator = fsp.watch(W);
  const reader = (async () => {
    for await (const event of iterator) {
      if (event.filename.startsWith("mark-")) break;
      events.push(event.eventType + ":" + event.filename + ":" + Object.keys(event));
    }
  })();
  await new Promise((resolve) => setImmediate(resolve));
  fs.writeFileSync(W + "/p.txt", "x");
  fs.mkdirSync(W + "/mark-iter");
  await reader;
  console.log([...new Set(events)].sort().join(" "));
  console.log(JSON.stringify(await iterator.next()));
  const abortIter = new AbortController();
  const pending = (async () => {
    try { for await (const event of fsp.watch(W, { signal: abortIter.signal })) void event; } catch (e) { return e.name + " " + e.code; }
  })();
  await new Promise((resolve) => setImmediate(resolve));
  abortIter.abort();
  console.log(await pending);
  for (const [label, run] of [
    ["missing", () => fsp.watch(W + "/missing").next()],
    ["bad path", () => fsp.watch(5).next()],
    ["bad overflow", () => fsp.watch(W, { overflow: "x" }).next()],
    ["bad persistent", () => fsp.watch(W, { persistent: "x" }).next()],
  ]) {
    await run().then(() => console.log(label, "resolved"), (e) => console.log(label, e.code, norm(e.message)));
  }
  dir.watcher.close();
  first.watcher.close();
  console.log("done");
}
main();
