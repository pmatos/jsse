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
// #1218: Node's errno errors (ENOENT, EEXIST, ENOTDIR, EISDIR, EACCES, ENOTEMPTY,
// ELOOP) for the whole synchronous surface, as a matrix over operations.
const root = process.getuid && process.getuid() === 0;
fs.writeFileSync(p("file"), "x");
fs.mkdirSync(p("dir"));
fs.writeFileSync(p("dir/inner"), "i");
fs.mkdirSync(p("emptydir"));
const nonRoot = (fn) => () => { if (root) throw Object.assign(new Error("EACCES: skipped as root"), { code: "EACCES" }); return fn(); };
function reseed() {
  for (const n of ["renamed", "hardlink", "copied", "cpd"]) {
    try { fs.rmSync(p(n), { recursive: true, force: true }); } catch (e) {}
  }
  const lacks = (n) => { try { fs.lstatSync(p(n)); return false; } catch (e) { return true; } };
  const make = (n, f) => { if (lacks(n)) { try { f(); } catch (e) {} } };
  make("file", () => fs.writeFileSync(p("file"), "x"));
  make("dir", () => { fs.mkdirSync(p("dir")); fs.writeFileSync(p("dir/inner"), "i"); });
  make("emptydir", () => fs.mkdirSync(p("emptydir")));
  make("target.txt", () => fs.writeFileSync(p("target.txt"), "target"));
  make("real-dir", () => fs.mkdirSync(p("real-dir")));
  make("link-file", () => fs.symlinkSync("target.txt", p("link-file")));
  make("link-dir", () => fs.symlinkSync("real-dir", p("link-dir")));
  make("dangling", () => fs.symlinkSync("missing", p("dangling")));
  make("loop", () => fs.symlinkSync("loop", p("loop")));
}
const ops = {
  open: (x) => fs.openSync(x, "r"),
  openW: (x) => fs.openSync(x, "w"),
  openWX: (x) => fs.openSync(x, "wx"),
  readFile: (x) => fs.readFileSync(x),
  writeFile: (x) => fs.writeFileSync(x, "y"),
  stat: (x) => fs.statSync(x),
  lstat: (x) => fs.lstatSync(x),
  statBigint: (x) => fs.statSync(x, { bigint: true }),
  access: (x) => fs.accessSync(x),
  readdir: (x) => fs.readdirSync(x),
  readdirTypes: (x) => fs.readdirSync(x, { withFileTypes: true }),
  readdirRecursive: (x) => fs.readdirSync(x, { recursive: true }),
  opendir: (x) => fs.opendirSync(x),
  mkdir: (x) => fs.mkdirSync(x),
  mkdirRecursive: (x) => fs.mkdirSync(x, { recursive: true }),
  rmdir: (x) => fs.rmdirSync(x),
  unlink: (x) => fs.unlinkSync(x),
  rm: (x) => fs.rmSync(x),
  rmRecursive: (x) => fs.rmSync(x, { recursive: true }),
  readlink: (x) => fs.readlinkSync(x),
  realpath: (x) => fs.realpathSync(x),
  realpathNative: (x) => fs.realpathSync.native(x),
  chmod: (x) => fs.chmodSync(x, 0o755),
  chown: (x) => fs.chownSync(x, -1, -1),
  lchown: (x) => fs.lchownSync(x, -1, -1),
  utimes: (x) => fs.utimesSync(x, 1, 1),
  lutimes: (x) => fs.lutimesSync(x, 1, 1),
  truncate: (x) => fs.truncateSync(x, 0),
  statfs: (x) => fs.statfsSync(x),
  mkdtemp: (x) => fs.mkdtempSync(x + "/t-"),
  renameFrom: (x) => fs.renameSync(x, p("renamed")),
  renameTo: (x) => fs.renameSync(p("file"), x),
  symlinkAt: (x) => fs.symlinkSync("t", x),
  linkFrom: (x) => fs.linkSync(x, p("hardlink")),
  linkTo: (x) => fs.linkSync(p("file"), x),
  copyFileFrom: (x) => fs.copyFileSync(x, p("copied")),
  copyFileTo: (x) => fs.copyFileSync(p("file"), x),
  copyFileToExcl: (x) => fs.copyFileSync(p("file"), x, fs.constants.COPYFILE_EXCL),
  cpFrom: (x) => fs.cpSync(x, p("cpd")),
  cpTo: (x) => fs.cpSync(p("file"), x),
};
const targets = {
  missing: p("missing"),
  "missing/child": p("missing/child"),
  "file/child": p("file/child"),
  file: p("file"),
  dir: p("dir"),
  emptydir: p("emptydir"),
  "link-file": p("link-file"),
  "link-dir": p("link-dir"),
  dangling: p("dangling"),
  "dangling/child": p("dangling/child"),
  loop: p("loop"),
  "loop/child": p("loop/child"),
  long: p("n".repeat(300)),
};
for (const [oname, op] of Object.entries(ops)) {
  for (const [tname, target] of Object.entries(targets)) {
    t(oname + " " + tname, () => {
      let r;
      try { r = op(target); } finally { reseed(); }
      return r === undefined ? undefined : "ok";
    });
  }
}
t("ENOTEMPTY rmdir", () => fs.rmdirSync(p("dir")));
t("ENOTEMPTY rename over", () => fs.renameSync(p("emptydir"), p("dir")));
t("ENOTEMPTY rename over file", () => fs.renameSync(p("dir"), p("file")));
t("EISDIR rename file over dir", () => fs.renameSync(p("file"), p("dir")));
t("EACCES readFile", nonRoot(() => { fs.writeFileSync(p("noread"), "x", { mode: 0o000 }); return fs.readFileSync(p("noread")); }));
t("EACCES writeFile", nonRoot(() => { fs.writeFileSync(p("nowrite"), "x", { mode: 0o444 }); return fs.writeFileSync(p("nowrite"), "y"); }));
t("EACCES readdir", nonRoot(() => { fs.mkdirSync(p("nolist"), { mode: 0o000 }); try { return fs.readdirSync(p("nolist")); } finally { fs.chmodSync(p("nolist"), 0o755); } }));
t("EACCES opendir", nonRoot(() => { fs.mkdirSync(p("nolist2"), { mode: 0o000 }); try { return fs.opendirSync(p("nolist2")); } finally { fs.chmodSync(p("nolist2"), 0o755); } }));
t("EACCES mkdir", nonRoot(() => { fs.mkdirSync(p("nomk"), { mode: 0o555 }); try { return fs.mkdirSync(p("nomk/x")); } finally { fs.chmodSync(p("nomk"), 0o755); } }));
t("EACCES mkdir recursive", nonRoot(() => { fs.mkdirSync(p("nomk2"), { mode: 0o555 }); try { return fs.mkdirSync(p("nomk2/x/y"), { recursive: true }); } finally { fs.chmodSync(p("nomk2"), 0o755); } }));
t("EACCES access", nonRoot(() => { fs.writeFileSync(p("noacc"), "x", { mode: 0o000 }); return fs.accessSync(p("noacc"), fs.constants.R_OK); }));
t("EACCES unlink", nonRoot(() => { fs.mkdirSync(p("nounl"), { mode: 0o755 }); fs.writeFileSync(p("nounl/f"), "x"); fs.chmodSync(p("nounl"), 0o555); try { return fs.unlinkSync(p("nounl/f")); } finally { fs.chmodSync(p("nounl"), 0o755); } }));
t("EACCES copyFile", nonRoot(() => { fs.writeFileSync(p("nocopy"), "x", { mode: 0o000 }); return fs.copyFileSync(p("nocopy"), p("nocopy2")); }));
t("EACCES copyFile dest", nonRoot(() => { fs.mkdirSync(p("nocopyd"), { mode: 0o555 }); try { return fs.copyFileSync(p("file"), p("nocopyd/x")); } finally { fs.chmodSync(p("nocopyd"), 0o755); } }));
t("EACCES symlink", nonRoot(() => { fs.mkdirSync(p("nosym"), { mode: 0o555 }); try { return fs.symlinkSync("t", p("nosym/x")); } finally { fs.chmodSync(p("nosym"), 0o755); } }));
t("EACCES cp", nonRoot(() => { fs.mkdirSync(p("nocp"), { mode: 0o555 }); try { return fs.cpSync(p("file"), p("nocp/x")); } finally { fs.chmodSync(p("nocp"), 0o755); } }));
t("EACCES chmod", nonRoot(() => { fs.mkdirSync(p("nochm"), { mode: 0o000 }); try { return fs.chmodSync(p("nochm/x"), 0o644); } finally { fs.chmodSync(p("nochm"), 0o755); } }));
t("EACCES truncate", nonRoot(() => { fs.writeFileSync(p("notr"), "x", { mode: 0o444 }); return fs.truncateSync(p("notr"), 0); }));
