const fs = require("fs");
const fsp = require("fs/promises");
const D = process.env.FS_ORACLE_DIR;
if (!D) throw new Error("FS_ORACLE_DIR is not set");
const norm = (s) => String(s).split(D).join("<D>");
function show(v) {
  if (v === undefined) return "undefined";
  if (Buffer.isBuffer(v)) return "buf:" + v.toString("hex");
  if (typeof v === "string") return JSON.stringify(v);
  if (Array.isArray(v)) return "[" + v.map(show).join(",") + "]";
  if (v && typeof v === "object" && typeof v.isFile === "function") {
    return v.name !== undefined ? "dirent:" + v.name + ":" + v.isFile() + v.isDirectory() + v.isSymbolicLink() : "stats:" + [v.isFile(), v.isDirectory(), v.isSymbolicLink(), v.size, (v.mode & 0o777).toString(8)];
  }
  return String(typeof v === "object" ? JSON.stringify(v) : v);
}
function fail(e) {
  return "!! " + e.name + " " + e.code + " " + e.errno + " " + e.syscall + " " + norm(e.path) + " " + norm(e.dest) + " " + norm(e.message);
}
const p = (name) => D + "/" + name;

// Runs one operation through its synchronous, callback and promise forms and
// prints each outcome plus whether the three agree. `setup` runs before each
// form so the forms see the same tree.
async function t(label, sync, callback, promise, setup) {
  const out = {};
  if (setup) setup();
  try { out.sync = "=> " + norm(show(sync())); } catch (e) { out.sync = fail(e); }
  if (setup) setup();
  out.callback = await new Promise((resolve) => {
    let called = 0;
    try {
      callback((e, value) => {
        if (called++) return resolve("called twice");
        resolve(e ? fail(e) : "=> " + norm(show(value)));
      });
    } catch (e) {
      resolve("threw " + fail(e));
    }
  });
  if (setup) setup();
  try { out.promise = "=> " + norm(show(await promise())); } catch (e) { out.promise = fail(e); }
  for (const k of ["sync", "callback", "promise"]) console.log(label, k, out[k]);
  console.log(label, "forms agree:", out.sync === out.callback && out.sync === out.promise);
}
const reset = () => {
  fs.rmSync(p("w"), { recursive: true, force: true });
  fs.mkdirSync(p("w"));
  fs.writeFileSync(p("w/a"), "alpha");
  fs.writeFileSync(p("w/b"), "bravo!");
  fs.mkdirSync(p("w/sub"));
  fs.writeFileSync(p("w/sub/c"), "c");
};

(async () => {
  reset();
  await t("open close", () => { const fd = fs.openSync(p("w/a"), "r"); fs.closeSync(fd); return typeof fd; }, (cb) => fs.open(p("w/a"), "r", (e, fd) => e ? cb(e) : fs.close(fd, (e2) => cb(e2, typeof fd))), () => fsp.open(p("w/a"), "r").then((fh) => fh.close().then(() => typeof fh.fd)).then(() => "number"));
  await t("close no callback", () => fs.closeSync(fs.openSync(p("w/a"), "r")), (cb) => { fs.close(fs.openSync(p("w/a"), "r")); setTimeout(() => cb(null), 30); }, () => new Promise((res) => { fs.close(fs.openSync(p("w/a"), "r")); setTimeout(res, 30); }));
  await t("open missing", () => fs.openSync(p("w/nope"), "r"), (cb) => fs.open(p("w/nope"), "r", cb), () => fsp.open(p("w/nope"), "r"));
  await t("open excl", () => fs.openSync(p("w/a"), "wx"), (cb) => fs.open(p("w/a"), "wx", cb), () => fsp.open(p("w/a"), "wx"), reset);
  await t("close bad fd", () => fs.closeSync(9999), (cb) => fs.close(9999, cb), () => fsp.open(p("w/a"), "r").then((fh) => fh.close().then(() => fh.close())).then(() => { const e = new Error("EBADF: bad file descriptor, close"); e.code = "EBADF"; throw e; }));
  await t("read", () => { const fd = fs.openSync(p("w/a"), "r"); const b = Buffer.alloc(5); const n = fs.readSync(fd, b, 0, 5, 0); fs.closeSync(fd); return [n, b]; }, (cb) => fs.open(p("w/a"), "r", (e, fd) => { const b = Buffer.alloc(5); fs.read(fd, b, 0, 5, 0, (e2, n, buf) => { fs.closeSync(fd); cb(e2, [n, buf]); }); }), async () => { const fh = await fsp.open(p("w/a"), "r"); const { bytesRead, buffer } = await fh.read(Buffer.alloc(5), 0, 5, 0); await fh.close(); return [bytesRead, buffer]; });
  await t("read position", () => { const fd = fs.openSync(p("w/a"), "r"); const b = Buffer.alloc(3); const n = fs.readSync(fd, b, 0, 3, 2); fs.closeSync(fd); return [n, b]; }, (cb) => fs.open(p("w/a"), "r", (e, fd) => { const b = Buffer.alloc(3); fs.read(fd, b, 0, 3, 2, (e2, n, buf) => { fs.closeSync(fd); cb(e2, [n, buf]); }); }), async () => { const fh = await fsp.open(p("w/a"), "r"); const { bytesRead, buffer } = await fh.read(Buffer.alloc(3), 0, 3, 2); await fh.close(); return [bytesRead, buffer]; });
  await t("read eof", () => { const fd = fs.openSync(p("w/a"), "r"); const b = Buffer.alloc(3); const n = fs.readSync(fd, b, 0, 3, 100); fs.closeSync(fd); return [n, b]; }, (cb) => fs.open(p("w/a"), "r", (e, fd) => { const b = Buffer.alloc(3); fs.read(fd, b, 0, 3, 100, (e2, n, buf) => { fs.closeSync(fd); cb(e2, [n, buf]); }); }), async () => { const fh = await fsp.open(p("w/a"), "r"); const { bytesRead, buffer } = await fh.read(Buffer.alloc(3), 0, 3, 100); await fh.close(); return [bytesRead, buffer]; });
  await t("read options", () => { const fd = fs.openSync(p("w/a"), "r"); const b = Buffer.alloc(4); const n = fs.readSync(fd, b, { offset: 1, length: 2, position: 0 }); fs.closeSync(fd); return [n, b]; }, (cb) => fs.open(p("w/a"), "r", (e, fd) => { const b = Buffer.alloc(4); fs.read(fd, b, { offset: 1, length: 2, position: 0 }, (e2, n, buf) => { fs.closeSync(fd); cb(e2, [n, buf]); }); }), async () => { const fh = await fsp.open(p("w/a"), "r"); const { bytesRead, buffer } = await fh.read(Buffer.alloc(4), { offset: 1, length: 2, position: 0 }); await fh.close(); return [bytesRead, buffer]; });
  await t("read bad fd", () => fs.readSync(9999, Buffer.alloc(1), 0, 1, 0), (cb) => fs.read(9999, Buffer.alloc(1), 0, 1, 0, cb), () => new Promise((_, rej) => fs.read(9999, Buffer.alloc(1), 0, 1, 0, rej)));
  await t("write", () => { const fd = fs.openSync(p("w/n"), "w"); const n = fs.writeSync(fd, "hello", 0); fs.closeSync(fd); return [n, fs.readFileSync(p("w/n"), "utf8")]; }, (cb) => fs.open(p("w/n"), "w", (e, fd) => fs.write(fd, "hello", 0, (e2, n) => { fs.closeSync(fd); cb(e2, [n, fs.readFileSync(p("w/n"), "utf8")]); })), async () => { const fh = await fsp.open(p("w/n"), "w"); const { bytesWritten } = await fh.write("hello", 0); await fh.close(); return [bytesWritten, fs.readFileSync(p("w/n"), "utf8")]; }, reset);
  await t("write buffer", () => { const fd = fs.openSync(p("w/n"), "w"); const n = fs.writeSync(fd, Buffer.from("abcdef"), 1, 3, 0); fs.closeSync(fd); return [n, fs.readFileSync(p("w/n"), "utf8")]; }, (cb) => fs.open(p("w/n"), "w", (e, fd) => fs.write(fd, Buffer.from("abcdef"), 1, 3, 0, (e2, n) => { fs.closeSync(fd); cb(e2, [n, fs.readFileSync(p("w/n"), "utf8")]); })), async () => { const fh = await fsp.open(p("w/n"), "w"); const { bytesWritten } = await fh.write(Buffer.from("abcdef"), 1, 3, 0); await fh.close(); return [bytesWritten, fs.readFileSync(p("w/n"), "utf8")]; }, reset);
  await t("ftruncate fsync", () => { const fd = fs.openSync(p("w/a"), "r+"); fs.ftruncateSync(fd, 1); fs.fsyncSync(fd); fs.closeSync(fd); return fs.readFileSync(p("w/a"), "utf8"); }, (cb) => fs.open(p("w/a"), "r+", (e, fd) => fs.ftruncate(fd, 1, () => fs.fsync(fd, () => fs.fdatasync(fd, (e2) => { fs.closeSync(fd); cb(e2, fs.readFileSync(p("w/a"), "utf8")); })))), async () => { const fh = await fsp.open(p("w/a"), "r+"); await fh.truncate(1); await fh.sync(); await fh.datasync(); await fh.close(); return fs.readFileSync(p("w/a"), "utf8"); }, reset);
  await t("fstat", () => { const fd = fs.openSync(p("w/a"), "r"); const s = fs.fstatSync(fd); fs.closeSync(fd); return s; }, (cb) => fs.open(p("w/a"), "r", (e, fd) => fs.fstat(fd, (e2, s) => { fs.closeSync(fd); cb(e2, s); })), async () => { const fh = await fsp.open(p("w/a"), "r"); const s = await fh.stat(); await fh.close(); return s; });
  await t("handle readFile", () => fs.readFileSync(p("w/a"), "utf8"), (cb) => fs.readFile(p("w/a"), "utf8", cb), async () => { const fh = await fsp.open(p("w/a"), "r"); const s = await fh.readFile("utf8"); await fh.close(); return s; });
  await t("handle writeFile", () => { fs.writeFileSync(p("w/n"), "via handle"); return fs.readFileSync(p("w/n"), "utf8"); }, (cb) => fs.writeFile(p("w/n"), "via handle", (e) => cb(e, fs.readFileSync(p("w/n"), "utf8"))), async () => { const fh = await fsp.open(p("w/n"), "w"); await fh.writeFile("via handle"); await fh.close(); return fs.readFileSync(p("w/n"), "utf8"); }, reset);
  await t("opendir", () => { const d = fs.opendirSync(p("w")); const names = []; let e; while ((e = d.readSync())) names.push(e.name); d.closeSync(); return names.sort(); }, (cb) => fs.opendir(p("w"), async (err, d) => { if (err) return cb(err); const names = []; for await (const e of d) names.push(e.name); cb(null, names.sort()); }), async () => { const d = await fsp.opendir(p("w")); const names = []; for await (const e of d) names.push(e.name); return names.sort(); });
  await t("opendir missing", () => fs.opendirSync(p("w/nope")), (cb) => fs.opendir(p("w/nope"), cb), () => fsp.opendir(p("w/nope")));
  await t("streams", () => fs.readFileSync(p("w/a"), "utf8"), (cb) => { let s = ""; fs.createReadStream(p("w/a")).on("data", (c) => (s += c)).on("end", () => cb(null, s)).on("error", cb); }, async () => { let s = ""; for await (const c of fs.createReadStream(p("w/a"))) s += c; return s; });
  await t("write stream", () => { fs.writeFileSync(p("w/n"), "abcdef"); return fs.readFileSync(p("w/n"), "utf8"); }, (cb) => { const w = fs.createWriteStream(p("w/n")); w.write("abc"); w.end("def", () => cb(null, fs.readFileSync(p("w/n"), "utf8"))); w.on("error", cb); }, () => new Promise((res, rej) => { const w = fs.createWriteStream(p("w/n")); w.on("error", rej); w.write("abc"); w.end("def", () => res(fs.readFileSync(p("w/n"), "utf8"))); }), reset);
  await t("read stream range", () => fs.readFileSync(p("w/a"), "utf8").slice(1, 4), (cb) => { let s = ""; fs.createReadStream(p("w/a"), { start: 1, end: 3 }).on("data", (c) => (s += c)).on("end", () => cb(null, s)); }, async () => { let s = ""; for await (const c of fs.createReadStream(p("w/a"), { start: 1, end: 3 })) s += c; return s; });
  await t("read stream missing", () => fs.readFileSync(p("w/nope")), (cb) => { fs.createReadStream(p("w/nope")).on("error", (e) => cb(e)); }, async () => { for await (const c of fs.createReadStream(p("w/nope"))); });
  const collect = async (it) => { const r = []; for await (const x of it) r.push(x); return r.sort(); };
  await t("glob", () => fs.globSync("**/*", { cwd: p("w") }).sort(), (cb) => fs.glob("**/*", { cwd: p("w") }, (e, r) => cb(e, r && r.sort())), () => collect(fsp.glob("**/*", { cwd: p("w") })));
  await t("glob pattern", () => fs.globSync("*/c", { cwd: p("w") }).sort(), (cb) => fs.glob("*/c", { cwd: p("w") }, (e, r) => cb(e, r && r.sort())), () => collect(fsp.glob("*/c", { cwd: p("w") })));
  await t("glob exclude", () => fs.globSync("**/*", { cwd: p("w"), exclude: (n) => n === "sub" }).sort(), (cb) => fs.glob("**/*", { cwd: p("w"), exclude: (n) => n === "sub" }, (e, r) => cb(e, r && r.sort())), () => collect(fsp.glob("**/*", { cwd: p("w"), exclude: (n) => n === "sub" })));
  await t("glob types", () => fs.globSync("**/*", { cwd: p("w"), withFileTypes: true }).map((d) => d.name + d.isDirectory()).sort(), (cb) => fs.glob("**/*", { cwd: p("w"), withFileTypes: true }, (e, r) => cb(e, r && r.map((d) => d.name + d.isDirectory()).sort())), async () => (await collect(fsp.glob("**/*", { cwd: p("w"), withFileTypes: true }))).map((d) => d.name + d.isDirectory()).sort());
  await t("bad callback", () => { try { fs.readFile(p("w/a")); } catch (e) { return e.code + " " + e.message; } }, (cb) => { try { fs.readFile(p("w/a")); } catch (e) { cb(null, e.code + " " + e.message); } }, async () => { try { fs.readFile(p("w/a")); } catch (e) { return e.code + " " + e.message; } });
  await t("bad path", () => { try { fs.readFileSync({}); } catch (e) { return e.code; } }, (cb) => { try { fs.readFile({}, () => {}); } catch (e) { cb(null, e.code); } }, async () => { try { await fsp.readFile({}); } catch (e) { return e.code; } });
  await t("order", () => "sync", (cb) => { const log = []; process.nextTick(() => log.push("tick")); fs.stat(p("w/a"), () => { log.push("cb"); cb(null, log.join(",")); }); log.push("sync"); }, async () => "sync");
})().catch((e) => { console.log("FIXTURE FAILED", e); process.exitCode = 1; });
