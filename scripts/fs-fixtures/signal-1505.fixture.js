// Issue #1505: `options.signal` on every fs form. The callback and promise
// readFile/writeFile/appendFile, the FileHandle forms, writeFile of (async)
// iterables aborted between chunks, aborts before, during and after the open,
// a large write aborted between 512 KiB chunks, and createReadStream /
// createWriteStream. Each is cross-checked against Node byte for byte: which
// error (AbortError, ABORT_ERR, cause === signal.reason), whether the callback
// ran synchronously, what is left on disk and that no descriptor leaks.
// Every operation is awaited, so the output does not depend on pool order.
const fs = require("fs");
const fsp = fs.promises;
const D = process.env.FS_ORACLE_DIR;
if (!D) throw new Error("FS_ORACLE_DIR is not set");
const norm = (s) => String(s).split(D).join("<D>");
const p = (name) => D + "/" + name;
const log = (label, text) => console.log(label, text);
const CHUNK = 512 * 1024;

const reason = new Error("custom reason");
const fresh = () => {
  const ac = new AbortController();
  return ac;
};
const aborted = () => {
  const ac = fresh();
  ac.abort(reason);
  return ac;
};
const describe = (e, ac) => {
  if (e == null) return "ok";
  const tag = e.name + " " + e.code;
  const cause = ac && e.name === "AbortError" ? " cause=" + (e.cause === ac.signal.reason) : "";
  return norm(tag + cause + (e.name === "AbortError" ? "" : " " + e.message));
};
const shown = (v) => (Buffer.isBuffer(v) ? "buf" + v.length : typeof v === "string" && v.length > 40 ? "str" + v.length : JSON.stringify(v));
const fds = () => {
  try { return fs.readdirSync("/proc/self/fd").length; } catch { return -1; }
};
// A read that failed on an abort may still be closing its file; wait for it.
async function settled(before) {
  await new Promise((r) => setTimeout(r, 10));
  for (let i = 0; i < 100 && fds() > before; i++) await new Promise((r) => setTimeout(r, 5));
}
const state = (name) => {
  try {
    const st = fs.statSync(p(name));
    return "size=" + st.size;
  } catch (e) {
    return e.code;
  }
};
const reset = () => {
  fs.rmSync(p("w"), { recursive: true, force: true });
  fs.mkdirSync(p("w"), { recursive: true });
  fs.writeFileSync(p("w/a"), "alpha");
  fs.writeFileSync(p("w/big"), Buffer.alloc(CHUNK * 2 + 17, 0x61));
};

// A callback form: `start(cb, ac)` begins the operation; `during(ac)` runs
// right after the call returns (the abort goes here).
function viaCallback(label, start, during) {
  return new Promise((resolve) => {
    const ac = fresh();
    if (during === "pre") ac.abort(reason);
    let ret;
    let sync = true;
    let result;
    const before = fds();
    const done = (e, v) => {
      result = "sync=" + sync + " " + describe(e, ac) + (e == null && v !== undefined ? " value=" + shown(v) : "");
      settled(before).then(() => {
        log(label, result + " fds=" + (fds() - before));
        resolve();
      });
    };
    try {
      ret = start(done, ac);
    } catch (e) {
      log(label, "threw " + describe(e));
      resolve();
      return;
    }
    sync = false;
    if (during === "tick") ac.abort(reason);
    if (during === "nextTick") process.nextTick(() => ac.abort(reason));
    if (during === "immediate") setImmediate(() => ac.abort(reason));
    if (during === "timeout") setTimeout(() => ac.abort(reason), 5);
    void ret;
  });
}

async function viaPromise(label, start, during) {
  const ac = fresh();
  if (during === "pre") ac.abort(reason);
  const before = fds();
  let promise;
  try {
    promise = start(ac);
  } catch (e) {
    log(label, "threw " + describe(e));
    return;
  }
  if (during === "tick") ac.abort(reason);
  if (during === "microtask") Promise.resolve().then(() => ac.abort(reason));
  if (during === "nextTick") process.nextTick(() => ac.abort(reason));
  if (during === "immediate") setImmediate(() => ac.abort(reason));
  let outcome;
  try {
    const v = await promise;
    outcome = "ok" + (v === undefined ? "" : " value=" + shown(v));
  } catch (e) {
    outcome = describe(e, ac);
  }
  await settled(before);
  log(label, outcome + " fds=" + (fds() - before));
}

async function* chunks(ac, abortAfter, size) {
  for (let i = 0; i < 4; i++) {
    yield Buffer.alloc(size, 0x62 + i);
    if (i + 1 === abortAfter) ac.abort(reason);
  }
}

(async () => {
  const whens = ["none", "pre", "tick", "nextTick"];

  // Callback forms, aborted at each point.
  for (const when of whens) {
    reset();
    await viaCallback(`cb readFile path ${when}`, (cb, ac) => fs.readFile(p("w/a"), { signal: ac.signal }, cb), when);
    await viaCallback(`cb readFile big ${when}`, (cb, ac) => fs.readFile(p("w/big"), { signal: ac.signal }, cb), when);
    await viaCallback(`cb writeFile path ${when}`, (cb, ac) => fs.writeFile(p("w/new"), "data", { signal: ac.signal }, cb), when);
    log(`  new after ${when}`, state("w/new"));
    await viaCallback(`cb writeFile trunc ${when}`, (cb, ac) => fs.writeFile(p("w/a"), "data", { signal: ac.signal }, cb), when);
    log(`  a after ${when}`, state("w/a"));
    fs.writeFileSync(p("w/a"), "alpha");
    await viaCallback(`cb writeFile big ${when}`, (cb, ac) => fs.writeFile(p("w/out"), Buffer.alloc(CHUNK * 3, 0x63), { signal: ac.signal }, cb), when);
    log(`  out after ${when}`, state("w/out"));
    await viaCallback(`cb appendFile path ${when}`, (cb, ac) => fs.appendFile(p("w/a"), "+more", { signal: ac.signal }, cb), when);
    log(`  a after append ${when}`, state("w/a"));
    fs.writeFileSync(p("w/a"), "alpha");
    await viaCallback(`cb writeFile empty ${when}`, (cb, ac) => fs.writeFile(p("w/empty"), "", { signal: ac.signal }, cb), when);
    log(`  empty after ${when}`, state("w/empty"));
    fs.rmSync(p("w/empty"), { force: true });
    fs.rmSync(p("w/new"), { force: true });
    fs.rmSync(p("w/out"), { force: true });
  }

  // Callback forms on a caller-owned descriptor: it is never closed.
  for (const when of ["pre", "tick"]) {
    reset();
    const fd = fs.openSync(p("w/a"), "r+");
    await viaCallback(`cb writeFile fd ${when}`, (cb, ac) => fs.writeFile(fd, "XY", { signal: ac.signal }, cb), when);
    log("  fd still open", String(fs.fstatSync(fd).size));
    await viaCallback(`cb appendFile fd ${when}`, (cb, ac) => fs.appendFile(fd, "ZW", { signal: ac.signal }, cb), when);
    fs.closeSync(fd);
    log("  a after fd forms", JSON.stringify(fs.readFileSync(p("w/a"), "latin1")));
  }

  // A failing open still reports the open error, not an abort.
  reset();
  await viaCallback("cb readFile missing live", (cb, ac) => fs.readFile(p("w/nope"), { signal: ac.signal }, cb), "none");
  await viaCallback("cb writeFile missing dir live", (cb, ac) => fs.writeFile(p("w/nodir/x"), "x", { signal: ac.signal }, cb), "none");

  // Promise forms on a path.
  for (const when of ["none", "pre", "tick", "microtask", "nextTick"]) {
    reset();
    await viaPromise(`p readFile path ${when}`, (ac) => fsp.readFile(p("w/a"), { signal: ac.signal }), when);
    await viaPromise(`p readFile big ${when}`, (ac) => fsp.readFile(p("w/big"), { signal: ac.signal }), when);
    await viaPromise(`p writeFile path ${when}`, (ac) => fsp.writeFile(p("w/new"), "data", { signal: ac.signal }), when);
    log(`  new after ${when}`, state("w/new"));
    await viaPromise(`p writeFile trunc ${when}`, (ac) => fsp.writeFile(p("w/a"), "data", { signal: ac.signal }), when);
    log(`  a after ${when}`, state("w/a"));
    fs.writeFileSync(p("w/a"), "alpha");
    await viaPromise(`p writeFile big ${when}`, (ac) => fsp.writeFile(p("w/out"), Buffer.alloc(CHUNK * 3, 0x63), { signal: ac.signal }), when);
    log(`  out after ${when}`, state("w/out"));
    await viaPromise(`p appendFile path ${when}`, (ac) => fsp.appendFile(p("w/a"), "+more", { signal: ac.signal }), when);
    log(`  a after append ${when}`, state("w/a"));
    fs.writeFileSync(p("w/a"), "alpha");
    await viaPromise(`p writeFile empty ${when}`, (ac) => fsp.writeFile(p("w/empty"), "", { signal: ac.signal }), when);
    log(`  empty after ${when}`, state("w/empty"));
    fs.rmSync(p("w/empty"), { force: true });
    fs.rmSync(p("w/new"), { force: true });
    fs.rmSync(p("w/out"), { force: true });
  }

  // Promise forms on a FileHandle: fsp.f(handle, ...) and handle.f(...).
  for (const when of ["none", "pre", "tick", "microtask", "nextTick"]) {
    reset();
    let h = await fsp.open(p("w/a"), "r");
    await viaPromise(`p readFile(handle) ${when}`, (ac) => fsp.readFile(h, { signal: ac.signal }), when);
    await h.close();
    h = await fsp.open(p("w/a"), "r");
    await viaPromise(`handle.readFile ${when}`, (ac) => h.readFile({ signal: ac.signal }), when);
    await h.close();
    h = await fsp.open(p("w/big"), "r");
    await viaPromise(`handle.readFile big ${when}`, (ac) => h.readFile({ signal: ac.signal, encoding: "latin1" }), when);
    await h.close();
    h = await fsp.open(p("w/a"), "r");
    await h.close();
    h = await fsp.open(p("w/h"), "w");
    await viaPromise(`p writeFile(handle) ${when}`, (ac) => fsp.writeFile(h, "handle-data", { signal: ac.signal }), when);
    await h.close();
    log(`  h after ${when}`, state("w/h"));
    h = await fsp.open(p("w/h"), "w");
    await viaPromise(`handle.writeFile ${when}`, (ac) => h.writeFile("hd2", { signal: ac.signal }), when);
    await h.close();
    log(`  h after ${when}`, state("w/h"));
    h = await fsp.open(p("w/h"), "w");
    await viaPromise(`handle.writeFile big ${when}`, (ac) => h.writeFile(Buffer.alloc(CHUNK + 5, 0x64), { signal: ac.signal }), when);
    await h.close();
    log(`  h after big ${when}`, state("w/h"));
    h = await fsp.open(p("w/h"), "w");
    await viaPromise(`p appendFile(handle) ${when}`, (ac) => fsp.appendFile(h, "app", { signal: ac.signal }), when);
    await h.close();
    log(`  h after append ${when}`, state("w/h"));
    h = await fsp.open(p("w/h"), "w");
    await viaPromise(`handle.appendFile ${when}`, (ac) => h.appendFile("app2", { signal: ac.signal }), when);
    await h.close();
    log(`  h after append2 ${when}`, state("w/h"));
    fs.rmSync(p("w/h"), { force: true });
  }

  // writeFile of iterables, aborted between chunks.
  for (const after of [0, 1, 2, 4]) {
    reset();
    for (const size of [10, CHUNK + 3]) {
      await viaPromise(`p writeFile async-iterable size=${size} abort-after=${after}`,
        (ac) => fsp.writeFile(p("w/it"), chunks(ac, after, size), { signal: ac.signal }), "none");
      log("  it after", state("w/it"));
      await viaPromise(`p writeFile sync-iterable size=${size} abort-after=${after}`,
        (ac) => fsp.writeFile(p("w/it"), (function* () {
          for (let i = 0; i < 4; i++) {
            yield Buffer.alloc(size, 0x62 + i);
            if (i + 1 === after) ac.abort(reason);
          }
        })(), { signal: ac.signal }), "none");
      log("  it after", state("w/it"));
      const h = await fsp.open(p("w/it"), "w");
      await viaPromise(`handle.writeFile async-iterable size=${size} abort-after=${after}`,
        (ac) => h.writeFile(chunks(ac, after, size), { signal: ac.signal }), "none");
      await h.close();
      log("  it after", state("w/it"));
    }
  }

  // writeFile of strings in an iterable.
  reset();
  await viaPromise("p writeFile iterable strings", (ac) => fsp.writeFile(p("w/it"), ["ab", "cd", Buffer.from("ef")], { signal: ac.signal }), "none");
  log("  it after", JSON.stringify(fs.readFileSync(p("w/it"), "latin1")));

  // A large write aborted between 512 KiB chunks: the abort lands once some
  // bytes are on disk, so the file is a whole number of chunks and not all of it.
  const total = CHUNK * 160;
  const big = Buffer.alloc(total, 0x65);
  async function partial(label, run) {
    for (let attempt = 0; attempt < 4; attempt++) {
      fs.rmSync(p("w/part"), { force: true });
      const ac = fresh();
      const promise = run(ac);
      const watch = () => {
        let size = 0;
        try { size = fs.statSync(p("w/part")).size; } catch {}
        if (size > 0) ac.abort(reason);
        else setImmediate(watch);
      };
      setImmediate(watch);
      let outcome;
      try {
        await promise;
        outcome = "ok";
      } catch (e) {
        outcome = describe(e, ac);
      }
      const size = fs.statSync(p("w/part")).size;
      if (outcome === "ok") continue;
      log(label, outcome + " chunks=" + (size % CHUNK === 0) + " partial=" + (size > 0 && size < total));
      return;
    }
    log(label, "never observed a partial write");
  }
  reset();
  await partial("p writeFile big mid-abort", (ac) => fsp.writeFile(p("w/part"), big, { signal: ac.signal }));
  await partial("p appendFile big mid-abort", (ac) => fsp.appendFile(p("w/part"), big, { signal: ac.signal }));
  await partial("handle.writeFile big mid-abort", async (ac) => {
    const h = await fsp.open(p("w/part"), "w");
    try { await h.writeFile(big, { signal: ac.signal }); } finally { await h.close(); }
  });
  fs.rmSync(p("w/part"), { force: true });

  // Validation.
  reset();
  for (const [label, fn] of [
    ["cb readFile bad signal", () => fs.readFile(p("w/a"), { signal: {} }, () => {})],
    ["cb writeFile bad signal", () => fs.writeFile(p("w/a"), "x", { signal: 1 }, () => {})],
  ]) {
    try { fn(); log(label, "returned"); } catch (e) { log(label, describe(e)); }
  }
  for (const [label, fn] of [
    ["p readFile bad signal", () => fsp.readFile(p("w/a"), { signal: {} })],
    ["p writeFile bad signal", () => fsp.writeFile(p("w/a"), "x", { signal: 1 })],
    ["p appendFile bad signal", () => fsp.appendFile(p("w/a"), "x", { signal: "s" })],
    ["p readFile undefined signal", () => fsp.readFile(p("w/a"), { signal: undefined, encoding: "utf8" })],
  ]) {
    try { log(label, "ok " + JSON.stringify(await fn())); } catch (e) { log(label, describe(e)); }
  }
  {
    const h = await fsp.open(p("w/a"), "r");
    for (const [label, fn] of [
      ["handle.readFile bad signal", () => h.readFile({ signal: {} })],
      ["p readFile(handle) bad signal", () => fsp.readFile(h, { signal: 3 })],
    ]) {
      try { log(label, "ok " + JSON.stringify(await fn())); } catch (e) { log(label, describe(e)); }
    }
    await h.close();
    const w = await fsp.open(p("w/a"), "r+");
    for (const [label, fn] of [
      ["handle.writeFile bad signal", () => w.writeFile("x", { signal: {} })],
      ["handle.appendFile bad signal", () => w.appendFile("x", { signal: {} })],
    ]) {
      try { log(label, "ok " + JSON.stringify(await fn())); } catch (e) { log(label, describe(e)); }
    }
    await w.close();
  }

  // stat, lstat and fstat take a signal too: it fails the call as soon as it fires.
  reset();
  for (const when of ["none", "pre", "tick"]) {
    const fd = fs.openSync(p("w/a"), "r");
    await viaCallback(`cb stat ${when}`, (cb, ac) => fs.stat(p("w/a"), { signal: ac.signal }, (e, v) => cb(e, v && v.size)), when);
    await viaCallback(`cb lstat ${when}`, (cb, ac) => fs.lstat(p("w/a"), { signal: ac.signal }, (e, v) => cb(e, v && v.size)), when);
    await viaCallback(`cb fstat ${when}`, (cb, ac) => fs.fstat(fd, { signal: ac.signal }, (e, v) => cb(e, v && v.size)), when);
    await viaPromise(`p stat ${when}`, (ac) => fsp.stat(p("w/a"), { signal: ac.signal }).then((v) => v.size), when);
    await viaPromise(`p lstat ${when}`, (ac) => fsp.lstat(p("w/a"), { signal: ac.signal }).then((v) => v.size), when);
    const h = await fsp.open(p("w/a"), "r");
    await viaPromise(`handle.stat ${when}`, (ac) => h.stat({ signal: ac.signal }).then((v) => v.size), when);
    await h.close();
    fs.closeSync(fd);
  }
  for (const [label, fn] of [
    ["cb stat bad signal", () => fs.stat(p("w/a"), { signal: {} }, () => {})],
    ["cb lstat bad signal", () => fs.lstat(p("w/a"), { signal: 1 }, () => {})],
    ["cb stat bad path aborted", () => fs.stat(1.5, { signal: aborted().signal }, () => {})],
  ]) {
    try { fn(); log(label, "returned"); } catch (e) { log(label, describe(e)); }
  }

  // Streams.
  async function streamCase(label, make, when) {
    const ac = fresh();
    if (when === "pre") ac.abort(reason);
    const before = fds();
    const events = [];
    const stream = make(ac);
    stream.on("error", (e) => events.push("error:" + describe(e, ac)));
    stream.on("close", () => events.push("close"));
    // A stream whose signal fires before its open returns opens the file after
    // destroying itself, and Node then leaks the descriptor in about half the runs.
    stream.on("open", () => when !== "pre" && events.push("open"));
    stream.on("data", () => events.push("data"));
    if (when === "tick") ac.abort(reason);
    if (when === "open") stream.once("open", () => ac.abort(reason));
    if (when === "data") stream.once("data", () => ac.abort(reason));
    await new Promise((resolve) => {
      if (stream.destroyed && events.includes("close")) resolve();
      stream.once("close", resolve);
      if (when === "none") stream.once("end", () => stream.destroy());
      if (when === "none" && stream.writable) stream.end("tail");
    });
    await settled(before);
    log(label, [...new Set(events)].join(",") + " destroyed=" + stream.destroyed + (when === "pre" || when === "tick" ? "" : " fds=" + (fds() - before)));
  }
  reset();
  for (const when of ["none", "pre", "tick", "open", "data"]) {
    await streamCase(`createReadStream ${when}`, (ac) => fs.createReadStream(p("w/a"), { signal: ac.signal }), when);
    await streamCase(`createReadStream big ${when}`, (ac) => fs.createReadStream(p("w/big"), { signal: ac.signal, highWaterMark: 1024 }), when);
    if (when === "data") continue;
    await streamCase(`createWriteStream ${when}`, (ac) => fs.createWriteStream(p("w/ws"), { signal: ac.signal }), when);
    if (when === "none" || when === "open") log("  ws after", state("w/ws"));
  }
  {
    for (const when of ["pre", "tick"]) {
      const h = await fsp.open(p("w/a"), "r");
      await streamCase(`handle.createReadStream ${when}`, (ac) => h.createReadStream({ signal: ac.signal, autoClose: true }), when);
      log("  handle fd after", String(h.fd));
      const w = await fsp.open(p("w/ws"), "w");
      await streamCase(`handle.createWriteStream ${when}`, (ac) => w.createWriteStream({ signal: ac.signal, autoClose: true }), when);
      log("  handle fd after", String(w.fd));
    }
  }
  try { fs.createReadStream(p("w/a"), { signal: {} }); } catch (e) { log("createReadStream bad signal", describe(e)); }
  try { fs.createWriteStream(p("w/ws"), { signal: 1 }); } catch (e) { log("createWriteStream bad signal", describe(e)); }

  const hb = await fsp.open(p("w/a"), "r");
  await viaCallback("cb readFile encoding buffer", (cb, ac) => fs.readFile(p("w/a"), { encoding: "buffer", signal: ac.signal }, cb), "none");
  await viaPromise("p readFile encoding buffer", (ac) => fsp.readFile(p("w/a"), { encoding: "buffer", signal: ac.signal }), "none");
  await viaPromise("handle.readFile encoding buffer", (ac) => hb.readFile({ encoding: "buffer", signal: ac.signal }), "none");
  await hb.close();

  // A live signal that never fires does not change the result.
  reset();
  const live = fresh();
  log("live readFile", JSON.stringify(await fsp.readFile(p("w/a"), { signal: live.signal, encoding: "utf8" })));
  log("live readFile big", String((await fsp.readFile(p("w/big"), { signal: live.signal })).length));
  await fsp.writeFile(p("w/lw"), Buffer.alloc(CHUNK * 2 + 1, 0x66), { signal: live.signal });
  log("live writeFile big", state("w/lw"));
  await fsp.appendFile(p("w/lw"), "tail", { signal: live.signal });
  log("live appendFile", state("w/lw"));
  await viaCallback("live cb writeFile", (cb, ac) => fs.writeFile(p("w/lw"), "short", { signal: ac.signal }, cb), "none");
  log("live cb writeFile after", state("w/lw"));
})().catch((e) => { console.log("FIXTURE FAILED", e); process.exitCode = 1; });
