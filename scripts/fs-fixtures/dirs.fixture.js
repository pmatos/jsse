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
// #1218: readdir withFileTypes/recursive, mkdir recursive, mkdtemp, opendir, rmSync.
t("setup", () => {
  fs.mkdirSync(p("tree/a/b"), { recursive: true });
  fs.writeFileSync(p("tree/top.txt"), "t");
  fs.writeFileSync(p("tree/a/mid.txt"), "m");
  fs.writeFileSync(p("tree/a/b/leaf.txt"), "l");
  fs.mkdirSync(p("tree/z"));
  fs.symlinkSync("a", p("tree/linkdir"));
  fs.symlinkSync("top.txt", p("tree/linkfile"));
  fs.symlinkSync("nowhere", p("tree/dangling"));
});
t("readdir wft", () => fs.readdirSync(p("tree"), { withFileTypes: true }));
t("readdir wft flags", () => fs.readdirSync(p("tree"), { withFileTypes: true }).map((d) => d.name + ":" + [d.isFile(), d.isDirectory(), d.isSymbolicLink()].map(Number).join("")));
t("readdir wft parentPath", () => fs.readdirSync(p("tree"), { withFileTypes: true }).every((d) => d.parentPath === p("tree")));
t("readdir wft string opt", () => fs.readdirSync(p("tree"), "utf8"));
t("readdir wft buffer", () => fs.readdirSync(p("tree"), { withFileTypes: true, encoding: "buffer" }).map((d) => Buffer.isBuffer(d.name) + ":" + d.name.toString()));
t("readdir recursive", () => fs.readdirSync(p("tree"), { recursive: true }));
t("readdir recursive wft", () => fs.readdirSync(p("tree"), { recursive: true, withFileTypes: true }).map((d) => norm(d.parentPath) + "|" + d.name));
t("readdir recursive buffer", () => fs.readdirSync(p("tree"), { recursive: true, encoding: "buffer" }).length);
t("readdir recursive false", () => fs.readdirSync(p("tree"), { recursive: false }));
t("readdir recursive bad", () => fs.readdirSync(p("tree"), { recursive: "yes" }));
t("readdir recursive missing", () => fs.readdirSync(p("nope"), { recursive: true }));
t("readdir recursive file", () => fs.readdirSync(p("target.txt"), { recursive: true }));
t("readdir wft missing", () => fs.readdirSync(p("nope"), { withFileTypes: true }));
t("readdir wft file", () => fs.readdirSync(p("target.txt"), { withFileTypes: true }));
t("readdir bad options", () => fs.readdirSync(p("tree"), 5));
t("mkdir recursive new", () => fs.mkdirSync(p("m/n/o"), { recursive: true }));
t("mkdir recursive existing", () => fs.mkdirSync(p("m/n/o"), { recursive: true }));
t("mkdir recursive partial", () => fs.mkdirSync(p("m/n/q/r"), { recursive: true }));
t("mkdir recursive relative return", () => { process.chdir(D); const r = fs.mkdirSync("rel/x/y", { recursive: true }); return r; });
t("mkdir recursive file", () => fs.mkdirSync(p("target.txt"), { recursive: true }));
t("mkdir recursive under file", () => fs.mkdirSync(p("target.txt/x/y"), { recursive: true }));
t("mkdir recursive link to file", () => fs.mkdirSync(p("link-file"), { recursive: true }));
t("mkdir recursive link to dir", () => fs.mkdirSync(p("link-dir"), { recursive: true }));
t("mkdir recursive loop", () => fs.mkdirSync(p("loop/x"), { recursive: true }));
t("mkdir recursive dangling", () => fs.mkdirSync(p("dangling"), { recursive: true }));
t("mkdir recursive mode", () => { fs.mkdirSync(p("pm/a"), { recursive: true, mode: 0o700 }); return (fs.statSync(p("pm/a")).mode & 0o777).toString(8); });
t("mkdir recursive false", () => fs.mkdirSync(p("m/n/zz"), { recursive: false }));
t("mkdir recursive false missing parent", () => fs.mkdirSync(p("nope/x"), { recursive: false }));
t("mkdir recursive bad", () => fs.mkdirSync(p("bad"), { recursive: 1 }));
t("mkdir mode bad", () => fs.mkdirSync(p("bad"), { mode: "zz" }));
t("mkdir mode string", () => { fs.mkdirSync(p("ms"), "755"); return (fs.statSync(p("ms")).mode & 0o777).toString(8); });
t("mkdir undefined options", () => fs.mkdirSync(p("uo"), undefined));
t("mkdtemp", () => {
  const made = fs.mkdtempSync(p("tmp-"));
  return [norm(made).length, norm(made).startsWith("<D>/tmp-"), fs.statSync(made).isDirectory(), (fs.statSync(made).mode & 0o777).toString(8)];
});
t("mkdtemp buffer", () => { const b = fs.mkdtempSync(p("tmpb-"), "buffer"); return [Buffer.isBuffer(b), b.length - p("tmpb-").length]; });
t("mkdtemp options", () => fs.mkdtempSync(p("tmpo-"), { encoding: "utf8" }).length - p("tmpo-").length);
t("mkdtemp bad", () => fs.mkdtempSync(1));
t("mkdtemp missing dir", () => fs.mkdtempSync(p("nope/x-")));
t("mkdtemp file dir", () => fs.mkdtempSync(p("target.txt/x-")));
t("opendir", () => {
  const dir = fs.opendirSync(p("tree"));
  const seen = [];
  let e;
  while ((e = dir.readSync()) !== null) seen.push(e.name + ":" + e.isDirectory() + ":" + (e.parentPath === p("tree")));
  const after = dir.readSync();
  dir.closeSync();
  return [norm(dir.path), seen.sort(), after];
});
t("opendir closed read", () => { const d = fs.opendirSync(p("tree")); d.closeSync(); d.readSync(); });
t("opendir closed close", () => { const d = fs.opendirSync(p("tree")); d.closeSync(); d.closeSync(); });
t("opendir missing", () => fs.opendirSync(p("nope")));
t("opendir file", () => fs.opendirSync(p("target.txt")));
t("opendir bad bufferSize", () => fs.opendirSync(p("tree"), { bufferSize: 0 }));
t("opendir bad path", () => fs.opendirSync(5));
t("opendir class", () => { const d = fs.opendirSync(p("tree")); const r = [d instanceof fs.Dir, Object.keys(d).length, typeof d.readSync, typeof d.closeSync]; d.closeSync(); return r; });
t("rm file", () => { fs.writeFileSync(p("rm1"), "x"); fs.rmSync(p("rm1")); return fs.existsSync(p("rm1")); });
t("rm missing", () => fs.rmSync(p("nope")));
t("rm missing force", () => fs.rmSync(p("nope"), { force: true }));
t("rm missing recursive", () => fs.rmSync(p("nope"), { recursive: true }));
t("rm missing recursive force", () => fs.rmSync(p("nope"), { recursive: true, force: true }));
t("rm dir", () => fs.rmSync(p("tree")));
t("rm dir force", () => fs.rmSync(p("tree"), { force: true }));
t("rm empty dir", () => { fs.mkdirSync(p("emptyd")); return fs.rmSync(p("emptyd")); });
t("rm dir recursive", () => { fs.rmSync(p("tree"), { recursive: true }); return fs.existsSync(p("tree")); });
t("rm link", () => { fs.rmSync(p("link-dir")); return [fs.existsSync(p("link-dir")), fs.existsSync(p("real-dir"))]; });
t("rm link recursive", () => { fs.rmSync(p("link-file"), { recursive: true }); return [fs.existsSync(p("link-file")), fs.existsSync(p("target.txt"))]; });
t("rm dangling", () => { fs.rmSync(p("dangling")); return fs.existsSync(p("dangling")); });
t("rm under file", () => fs.rmSync(p("target.txt/x")));
t("rm under file force", () => fs.rmSync(p("target.txt/x"), { force: true }));
t("rm bad options", () => fs.rmSync(p("target.txt"), 1));
t("rm bad force", () => fs.rmSync(p("target.txt"), { force: 1 }));
t("rm bad recursive", () => fs.rmSync(p("target.txt"), { recursive: "y" }));
t("rm bad retryDelay", () => fs.rmSync(p("target.txt"), { retryDelay: -1 }));
t("rm bad maxRetries", () => fs.rmSync(p("target.txt"), { maxRetries: -1 }));
t("rm bad path", () => fs.rmSync(5));
t("rm EACCES", () => {
  if (process.getuid && process.getuid() === 0) throw Object.assign(new Error("EACCES: skipped as root"), { code: "EACCES" });
  fs.mkdirSync(p("ro")); fs.writeFileSync(p("ro/f"), "x"); fs.chmodSync(p("ro"), 0o555);
  try { fs.rmSync(p("ro/f")); } finally { fs.chmodSync(p("ro"), 0o755); }
});
t("rm recursive EACCES", () => {
  if (process.getuid && process.getuid() === 0) throw Object.assign(new Error("EACCES: skipped as root"), { code: "EACCES" });
  fs.mkdirSync(p("ro2")); fs.mkdirSync(p("ro2/s")); fs.writeFileSync(p("ro2/s/f"), "x"); fs.chmodSync(p("ro2/s"), 0o555);
  try { fs.rmSync(p("ro2"), { recursive: true }); } finally { fs.chmodSync(p("ro2/s"), 0o755); }
});
