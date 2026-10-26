const fs = typeof require === "function" ? require("fs") : __node_builtins.fs;
const D = process.env.FS_ORACLE_DIR;
if (!D) throw new Error("FS_ORACLE_DIR is not set");
const norm = (s) => String(s).split(D).join("<D>");
function show(v) {
  if (v === undefined) return "undefined";
  if (Buffer.isBuffer(v)) return "buf:" + v.toString("hex");
  if (typeof v === "string") return JSON.stringify(v);
  if (typeof v === "bigint") return v + "n";
  if (Array.isArray(v)) return "[" + v.map(show).join(",") + "]";
  if (v && v.constructor && v.constructor.name === "Dirent") return "Dirent(" + show(v.name) + "," + show(v.parentPath) + ")";
  return String(typeof v === "object" ? JSON.stringify(v) : v);
}
function t(label, fn) {
  try {
    console.log(label, "=>", norm(show(fn())));
  } catch (e) {
    console.log(label, "!!", e.name, e.code, e.errno, e.syscall, norm(e.path), norm(e.dest), norm(e.message), Object.keys(e).join());
  }
}
const p = (name) => D + "/" + name;
// #1218: Stats time fields (ms, ns, Date), BigIntStats, StatFs and Dirent.
fs.writeFileSync(p("s.txt"), "12345");
const T1 = 1700000000.25;
const T2 = 1234567890.123456;
t("utimes fractional", () => fs.utimesSync(p("s.txt"), T1, T2));
t("stat times", () => {
  const s = fs.statSync(p("s.txt"));
  return [s.atimeMs, s.mtimeMs, s.atime.toISOString(), s.mtime.toISOString(), s.atime instanceof Date, s.size, s.isFile(), s.isDirectory(), Number.isInteger(s.mode)];
});
t("stat times cached", () => {
  const s = fs.statSync(p("s.txt"));
  const first = s.mtime;
  return [first === s.mtime, Object.keys(s).join(","), Object.keys(s).includes("mtime")];
});
t("stat date setter", () => {
  const s = fs.statSync(p("s.txt"));
  s.mtime = new Date(42);
  return [s.mtime.getTime(), s.mtimeMs === fs.statSync(p("s.txt")).mtimeMs];
});
t("stat keys", () => Object.keys(fs.statSync(p("s.txt"))).join(","));
t("bigint stat", () => {
  const s = fs.statSync(p("s.txt"), { bigint: true });
  return [typeof s.mode, typeof s.size, s.size, s.atimeNs, s.mtimeNs, s.atimeMs, s.mtimeMs, s.mtime.toISOString(), s.isFile(), s.isDirectory(), s.isSymbolicLink()];
});
t("bigint keys", () => Object.keys(fs.statSync(p("s.txt"), { bigint: true })).join(","));
t("bigint lstat link", () => fs.lstatSync(p("link-file"), { bigint: true }).isSymbolicLink());
t("bigint fstat", () => { const fd = fs.openSync(p("s.txt")); const s = fs.fstatSync(fd, { bigint: true }); fs.closeSync(fd); return [s.size, typeof s.dev]; });
t("bigint noThrow", () => fs.statSync(p("nope"), { bigint: true, throwIfNoEntry: false }));
t("noThrow ENOTDIR", () => fs.statSync(p("target.txt/x"), { throwIfNoEntry: false }));
t("noThrow lstat", () => fs.lstatSync(p("nope"), { throwIfNoEntry: false }));
t("noThrow other error", () => fs.statSync(p("loop"), { throwIfNoEntry: false }));
t("stats types", () => [fs.statSync(D).isDirectory(), fs.statSync("/dev/null").isCharacterDevice(), fs.statSync(p("s.txt")).isBlockDevice(), fs.statSync(p("s.txt")).isFIFO(), fs.statSync(p("s.txt")).isSocket()]);
t("utimes after Date", () => {
  fs.utimesSync(p("s.txt"), new Date(5000), new Date(6500));
  const s = fs.statSync(p("s.txt"));
  return [s.atimeMs, s.mtimeMs, s.mtime.getTime()];
});
t("utimes pre-epoch", () => {
  fs.utimesSync(p("s.txt"), -0.5 + 0 * 1, 10);
  return fs.statSync(p("s.txt")).mtimeMs;
});
t("Stats class", () => {
  const s = fs.statSync(p("s.txt"));
  return [s instanceof fs.Stats, typeof fs.Stats, Object.getPrototypeOf(fs.Stats.prototype) !== Object.prototype];
});
t("statfs", () => {
  const s = fs.statfsSync(D);
  return [Object.keys(s).join(","), typeof s.type, typeof s.bsize, s.bsize > 0, s.blocks >= s.bfree, s.bfree >= s.bavail, s.constructor.name];
});
t("statfs bigint", () => { const s = fs.statfsSync(D, { bigint: true }); return [typeof s.bsize, typeof s.blocks, typeof s.type]; });
t("statfs missing", () => fs.statfsSync(p("nope")));
t("statfs bad", () => fs.statfsSync(1));
t("Dirent", () => {
  fs.mkdirSync(p("dd"));
  fs.writeFileSync(p("dd/f"), "f");
  fs.mkdirSync(p("dd/d"));
  fs.symlinkSync("f", p("dd/l"));
  return fs.readdirSync(p("dd"), { withFileTypes: true }).map((d) => [d.name, d.isFile(), d.isDirectory(), d.isSymbolicLink(), d.isFIFO(), d.isSocket(), d.isBlockDevice(), d.isCharacterDevice(), d.constructor.name, norm(d.parentPath), Object.keys(d).join("|")].join(":"));
});
t("Dirent class", () => typeof fs.Dirent + typeof fs.Dir);
