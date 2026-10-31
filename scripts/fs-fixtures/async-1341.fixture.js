// Issue #1341: the async fs follow-ups. Validation that throws synchronously,
// the multi-step callback forms (rm, truncate, realpath, recursive readdir),
// the metadata calls on the pool, readv/writev, the FileHandle methods,
// openAsBlob, `signal` and the util.promisify hooks, each cross-checked
// against Node byte for byte. Every operation is awaited, so the output does
// not depend on pool completion order.
const fs = require("fs");
const fsp = fs.promises;
const util = require("util");
const D = process.env.FS_ORACLE_DIR;
if (!D) throw new Error("FS_ORACLE_DIR is not set");
const norm = (s) => String(s).split(D).join("<D>").split(process.cwd()).join("<CWD>");
const p = (name) => D + "/" + name;
const fail = (e) => ("!! " + e.name + " " + e.code + " " + e.errno + " " + e.syscall + " " + norm(e.path) + " " + norm(e.dest) + " " + norm(e.message)).replace(/x-[A-Za-z0-9]{6}/g, "x-RANDOM");
function show(v) {
  if (v === undefined) return "undefined";
  if (Buffer.isBuffer(v)) return "buf:" + v.toString("hex");
  if (typeof v === "string") return JSON.stringify(norm(v));
  if (Array.isArray(v)) return "[" + v.map(show).join(",") + "]";
  if (v && typeof v === "object" && typeof v.isFile === "function") {
    return v.name !== undefined
      ? "dirent:" + v.name + ":" + norm(v.parentPath) + ":" + v.isFile() + v.isDirectory() + v.isSymbolicLink()
      : "stats";
  }
  return norm(typeof v === "object" ? JSON.stringify(v) : v);
}
const log = (label, text) => console.log(label, text);
const attempt = (label, fn) => {
  try { log(label, "=> " + show(fn())); } catch (e) { log(label, fail(e)); }
};
const viaCallback = (fn) => new Promise((resolve) => {
  try {
    fn((e, ...v) => resolve(e ? fail(e) : "=> " + show(v.length > 1 ? v : v[0])));
  } catch (e) {
    resolve("threw " + fail(e));
  }
});
const reset = () => {
  fs.rmSync(p("w"), { recursive: true, force: true });
  fs.mkdirSync(p("w/sub/deep"), { recursive: true });
  fs.writeFileSync(p("w/a"), "alpha");
  fs.writeFileSync(p("w/sub/c"), "c");
  fs.writeFileSync(p("w/sub/deep/d"), "d");
  fs.symlinkSync("sub", p("w/lnk"));
  fs.symlinkSync("a", p("w/lna"));
};
const noop = () => {};

(async () => {
  reset();
  // Validation is synchronous: these throw before any callback is queued.
  const thrown = {
    "rm path": () => fs.rm(123, noop),
    "rm option": () => fs.rm(p("w/a"), { recursive: "x" }, noop),
    "rm retryDelay": () => fs.rm(p("w/a"), { retryDelay: -1 }, noop),
    "cp path": () => fs.cp(1, 2, noop),
    "cp option": () => fs.cp(p("w/a"), p("w/b"), { force: 1 }, noop),
    "cp pair": () => fs.cp(p("w/a"), p("w/b"), { dereference: true, verbatimSymlinks: true }, noop),
    "readdir recursive": () => fs.readdir(p("w"), { recursive: "x" }, noop),
    "readdir encoding": () => fs.readdir(p("w"), { encoding: "nope", recursive: true }, noop),
    "truncate len": () => fs.truncate(p("w/a"), "y", noop),
    "truncate path": () => fs.truncate(1.5, noop),
    "chmod mode": () => fs.chmod(p("w/a"), "zz", noop),
    "utimes path": () => fs.utimes(null, 1, 2, noop),
    "mkdtemp prefix": () => fs.mkdtemp(null, noop),
    "link path": () => fs.link(1, p("w/x"), noop),
    "symlink path": () => fs.symlink(p("w/a"), 1, noop),
    "statfs path": () => fs.statfs(1, noop),
    "realpath options": () => fs.realpath(p("w/a"), 5, noop),
    "readv fd": () => fs.readv("x", [Buffer.alloc(1)], noop),
    "readv buffers": () => fs.readv(0, "x", noop),
    "readv item": () => fs.readv(0, [Buffer.alloc(1), "x"], noop),
    "readv cb": () => fs.readv(0, [Buffer.alloc(1)]),
    "writev buffers": () => fs.writev(0, {}, noop),
    "writev cb": () => fs.writev(0, [Buffer.alloc(1)], 0, 5),
    "readFile signal": () => fs.readFile(p("w/a"), { signal: {} }, noop),
    "writeFile signal": () => fs.writeFile(p("w/a"), "x", { signal: 1 }, noop),
  };
  for (const [label, fn] of Object.entries(thrown)) {
    try { fn(); log("sync " + label, "no throw"); } catch (e) { log("sync " + label, "throws " + e.code + " " + norm(e.message)); }
  }
  for (const [label, fn] of Object.entries({
    "realpath number": (cb) => fs.realpath(1, cb),
    "realpath object": (cb) => fs.realpath({}, cb),
    "realpath missing": (cb) => fs.realpath(p("w/nope/x"), cb),
    "realpath chain": (cb) => fs.realpath(p("w/lnk/deep/../c"), cb),
    "realpath native": (cb) => fs.realpath.native(p("w/lna"), cb),
  })) log("cb " + label, await viaCallback(fn));

  // Multi-step functions: success, failure and the result seen afterwards.
  const step = async (label, fn, after) => {
    log(label, await viaCallback(fn));
    if (after) attempt(label + " after", after);
    reset();
  };
  await step("rm recursive", (cb) => fs.rm(p("w"), { recursive: true }, cb), () => fs.existsSync(p("w")));
  await step("rm missing", (cb) => fs.rm(p("w/nope"), cb));
  await step("rm missing force", (cb) => fs.rm(p("w/nope"), { force: true }, cb));
  await step("rm dir", (cb) => fs.rm(p("w/sub"), cb));
  await step("rm file", (cb) => fs.rm(p("w/a"), cb), () => fs.existsSync(p("w/a")));
  await step("rm link", (cb) => fs.rm(p("w/lnk"), { recursive: true }, cb), () => [fs.existsSync(p("w/lnk")), fs.existsSync(p("w/sub/c"))]);
  await step("rm recursive missing", (cb) => fs.rm(p("w/nope"), { recursive: true }, cb));
  await step("rm recursive force", (cb) => fs.rm(p("w/nope"), { recursive: true, force: true }, cb));
  await step("rm file under file", (cb) => fs.rm(p("w/a/x"), cb));
  await step("rm file under file force", (cb) => fs.rm(p("w/a/x"), { force: true }, cb));
  await step("truncate", (cb) => fs.truncate(p("w/a"), 2, cb), () => fs.readFileSync(p("w/a"), "utf8"));
  await step("truncate extend", (cb) => fs.truncate(p("w/a"), 8, cb), () => fs.readFileSync(p("w/a"), "hex"));
  await step("truncate default", (cb) => fs.truncate(p("w/a"), cb), () => fs.statSync(p("w/a")).size);
  await step("truncate negative", (cb) => fs.truncate(p("w/a"), -4, cb), () => fs.statSync(p("w/a")).size);
  await step("truncate missing", (cb) => fs.truncate(p("w/nope"), 1, cb));
  await step("truncate dir", (cb) => fs.truncate(p("w/sub"), 1, cb));
  await step("readdir recursive", (cb) => fs.readdir(p("w"), { recursive: true }, cb));
  await step("readdir recursive types", (cb) => fs.readdir(p("w"), { recursive: true, withFileTypes: true }, cb));
  await step("readdir recursive buffer", (cb) => fs.readdir(p("w"), { recursive: true, encoding: "buffer" }, cb));
  await step("readdir recursive missing", (cb) => fs.readdir(p("w/nope"), { recursive: true }, cb));
  await step("readdir recursive file", (cb) => fs.readdir(p("w/a"), { recursive: true }, cb));
  await step("readdir recursive tree", (cb) => fs.readdir(D, { recursive: true }, cb));
  log("promise readdir recursive", show(await fsp.readdir(p("w"), { recursive: true })));
  log("promise rm", show(await fsp.rm(p("w/sub"), { recursive: true })));
  await fsp.rm(p("w/a/x")).catch((e) => log("promise rm error", fail(e)));
  reset();

  // Metadata calls (on the pool): success and the error shape.
  const mode = () => (fs.statSync(p("w/a")).mode & 0o777).toString(8);
  await step("chmod", (cb) => fs.chmod(p("w/a"), 0o640, cb), mode);
  await step("chmod missing", (cb) => fs.chmod(p("w/nope"), 0o640, cb));
  await step("lchmod absent", (cb) => cb(null, typeof fs.lchmod));
  await step("chown self", (cb) => fs.chown(p("w/a"), fs.statSync(p("w/a")).uid, fs.statSync(p("w/a")).gid, cb));
  await step("chown missing", (cb) => fs.chown(p("w/nope"), 1, 1, cb));
  await step("lchown self", (cb) => fs.lchown(p("w/lna"), fs.statSync(p("w/a")).uid, fs.statSync(p("w/a")).gid, cb));
  await step("utimes", (cb) => fs.utimes(p("w/a"), 1000, 2000, cb), () => [fs.statSync(p("w/a")).atimeMs, fs.statSync(p("w/a")).mtimeMs]);
  await step("utimes missing", (cb) => fs.utimes(p("w/nope"), 1, 2, cb));
  await step("lutimes", (cb) => fs.lutimes(p("w/lna"), 3000, 4000, cb), () => fs.lstatSync(p("w/lna")).mtimeMs);
  await step("link", (cb) => fs.link(p("w/a"), p("w/hard"), cb), () => fs.statSync(p("w/a")).nlink);
  await step("link exists", (cb) => fs.link(p("w/a"), p("w/sub/c"), cb));
  await step("link missing", (cb) => fs.link(p("w/nope"), p("w/x"), cb));
  await step("symlink", (cb) => fs.symlink("a", p("w/new"), cb), () => fs.readlinkSync(p("w/new")));
  await step("symlink exists", (cb) => fs.symlink("a", p("w/a"), cb));
  await step("mkdtemp", (cb) => fs.mkdtemp(p("w/t-"), (e, d) => cb(e, e ? undefined : [norm(d).length, fs.statSync(d).isDirectory()])));
  await step("mkdtemp buffer", (cb) => fs.mkdtemp(p("w/t-"), "buffer", (e, d) => cb(e, e ? undefined : Buffer.isBuffer(d))));
  await step("mkdtemp missing", (cb) => fs.mkdtemp(p("nope/x-"), cb));
  await step("statfs", (cb) => fs.statfs(p("w"), (e, s) => cb(e, e ? undefined : Object.keys(s).join(",") + ":" + (s.bsize > 0))));
  await step("statfs bigint", (cb) => fs.statfs(p("w"), { bigint: true }, (e, s) => cb(e, e ? undefined : typeof s.bsize)));
  await step("statfs missing", (cb) => fs.statfs(p("w/nope"), cb));
  log("promise chmod", show(await fsp.chmod(p("w/a"), 0o600)));
  await fsp.link(p("w/nope"), p("w/x")).catch((e) => log("promise link error", fail(e)));

  // readv / writev
  const fd = fs.openSync(p("w/a"), "r+");
  const bufs = () => [Buffer.alloc(3), Buffer.alloc(4), Buffer.alloc(0), Buffer.alloc(5)];
  const text = (list) => list.map((b) => b.toString("hex"));
  attempt("readvSync", () => { const b = bufs(); return [fs.readvSync(fd, b), text(b)]; });
  attempt("readvSync pos", () => { const b = bufs(); return [fs.readvSync(fd, b, 1), text(b)]; });
  attempt("readvSync eof", () => { const b = bufs(); return [fs.readvSync(fd, b, 100), text(b)]; });
  attempt("readvSync empty", () => fs.readvSync(fd, []));
  attempt("readvSync view", () => fs.readvSync(fd, [new DataView(new ArrayBuffer(2))], 0));
  attempt("readvSync bad", () => fs.readvSync(fd, "x"));
  attempt("readvSync item", () => fs.readvSync(fd, [Buffer.alloc(1), 1]));
  attempt("readvSync fd", () => fs.readvSync(-1, [Buffer.alloc(1)]));
  attempt("readvSync ebadf", () => fs.readvSync(9999, [Buffer.alloc(1)]));
  attempt("writevSync", () => fs.writevSync(fd, [Buffer.from("XY"), Buffer.from("Z")], 0));
  attempt("writevSync typed", () => fs.writevSync(fd, [new Uint8Array([65, 66]), new Uint16Array([0x4443])], 3));
  attempt("writevSync empty", () => fs.writevSync(fd, []));
  attempt("writevSync bad", () => fs.writevSync(fd, {}));
  attempt("writevSync ebadf", () => fs.writevSync(9999, [Buffer.from("x")]));
  attempt("content", () => fs.readFileSync(p("w/a"), "latin1"));
  for (const [label, fn] of Object.entries({
    "readv": (cb) => { const b = bufs(); fs.readv(fd, b, 0, (e, n, list) => cb(e, [n, text(list), list === b])); },
    "readv no position": (cb) => { const b = bufs(); fs.readv(fd, b, (e, n, list) => cb(e, [n, text(list), list === b])); },
    "readv pos": (cb) => { const b = bufs(); fs.readv(fd, b, 2, (e, n, list) => cb(e, [n, text(list)])); },
    "readv empty": (cb) => fs.readv(fd, [], (e, n, list) => cb(e, [n, list.length])),
    "readv ebadf": (cb) => fs.readv(9999, [Buffer.alloc(1)], (e, n, list) => (e ? cb(e) : cb(null, [n, list.length]))),
    "readv ebadf args": (cb) => fs.readv(9999, [Buffer.alloc(1)], (...a) => cb(null, [a.length, a[1], Array.isArray(a[2])])),
    "writev": (cb) => fs.writev(fd, [Buffer.from("12"), Buffer.from("345")], 0, (e, n, list) => cb(e, [n, list.length])),
    "writev empty": (cb) => fs.writev(fd, [], (e, n, list) => cb(e, [n, list.length])),
    "writev ebadf": (cb) => fs.writev(9999, [Buffer.from("x")], (...a) => cb(null, [a.length, a[1], Array.isArray(a[2])])),
  })) log("cb " + label, await viaCallback(fn));
  log("content", show(fs.readFileSync(p("w/a"), "latin1")));
  fs.closeSync(fd);

  // util.promisify shapes
  const fd2 = fs.openSync(p("w/a"), "r+");
  const shape = async (label, fn) => {
    try { const r = await fn(); log(label, Object.keys(r).join(",") + " " + Object.values(r).map((v) => (typeof v === "number" ? v : Array.isArray(v) ? "array" : "buffer")).join(",")); } catch (e) { log(label, fail(e)); }
  };
  await shape("promisify read", () => util.promisify(fs.read)(fd2, Buffer.alloc(2), 0, 2, 0));
  await shape("promisify write", () => util.promisify(fs.write)(fd2, Buffer.from("ab"), 0, 2, 0));
  await shape("promisify readv", () => util.promisify(fs.readv)(fd2, [Buffer.alloc(2)], 0));
  await shape("promisify writev", () => util.promisify(fs.writev)(fd2, [Buffer.from("ab")], 0));
  log("promisify exists", [await util.promisify(fs.exists)(p("w/a")), await util.promisify(fs.exists)(p("w/nope"))].join(","));
  log("custom symbols", [fs.exists, fs.read, fs.write, fs.readv, fs.writev].map((f) => typeof f[util.promisify.custom]).join(","));
  fs.closeSync(fd2);

  // FileHandle
  const h = await fsp.open(p("w/a"), "r+");
  log("handle methods", ["readv", "writev", "readLines"].map((m) => typeof h[m]).join(","));
  log("handle readv", show(await h.readv([Buffer.alloc(2), Buffer.alloc(3)], 0).then((r) => [r.bytesRead, r.buffers.length, text(r.buffers)])));
  log("handle writev", show(await h.writev([Buffer.from("A"), Buffer.from("BC")], 0).then((r) => [r.bytesWritten, r.buffers.length])));
  await h.readv("x").catch((e) => log("handle readv bad", e.code));
  await h.writev([1]).catch((e) => log("handle writev bad", e.code));
  fs.writeFileSync(p("w/lines"), "one\ntwo\r\nthree");
  const lines = [];
  for await (const line of (await fsp.open(p("w/lines"))).readLines()) lines.push(line);
  log("handle readLines", show(lines));
  const part = [];
  const h3 = await fsp.open(p("w/lines"));
  for await (const line of h3.readLines({ start: 4, end: 6 })) part.push(line);
  log("handle readLines range", show(part));
  await h.close();

  // A stream holds a reference: close() resolves, the descriptor stays open
  // until the stream is done.
  const h4 = await fsp.open(p("w/lines"));
  const stream = h4.createReadStream({ encoding: undefined });
  const chunks = [];
  stream.on("data", (c) => chunks.push(c));
  const closed = h4.close();
  log("ref close pending", String(h4.fd !== -1));
  await new Promise((resolve) => stream.on("close", resolve));
  await closed;
  log("ref stream", show(Buffer.concat(chunks).toString()));
  log("ref after", String(h4.fd));
  await h4.close();

  // openAsBlob
  const blob = await fs.openAsBlob(p("w/lines"), { type: "text/plain" });
  log("blob", [blob.size, blob.type, await blob.text()].join("|"));
  try { await fs.openAsBlob(p("w/nope")); } catch (e) { log("blob missing", e.name + " " + e.code + " " + e.message); }
  try { fs.openAsBlob(1); } catch (e) { log("blob bad path", e.code); }
  try { fs.openAsBlob(p("w/a"), { type: 1 }); } catch (e) { log("blob bad type", e.code); }

  // signal
  const aborted = new AbortController();
  aborted.abort();
  log("cb readFile aborted", await viaCallback((cb) => fs.readFile(p("w/a"), { signal: aborted.signal }, cb)));
  log("cb writeFile aborted", await viaCallback((cb) => fs.writeFile(p("w/a"), "x", { signal: aborted.signal }, cb)));
  log("cb appendFile aborted", await viaCallback((cb) => fs.appendFile(p("w/a"), "x", { signal: aborted.signal }, cb)));
  await fsp.readFile(p("w/a"), { signal: aborted.signal }).catch((e) => log("promise readFile aborted", e.name + " " + e.code));
  await fsp.writeFile(p("w/a"), "x", { signal: aborted.signal }).catch((e) => log("promise writeFile aborted", e.name + " " + e.code));
  const late = new AbortController();
  const pending = fsp.readFile(p("w/a"), { signal: late.signal });
  late.abort();
  await pending.catch((e) => log("promise readFile late abort", e.name + " " + e.code));
  const live = new AbortController();
  log("readFile live signal", show(await fsp.readFile(p("w/lines"), { signal: live.signal, encoding: "utf8" })));
  log("content after abort", show(fs.readFileSync(p("w/a"), "latin1")));
})().catch((e) => { console.log("FIXTURE FAILED", e); process.exitCode = 1; });
