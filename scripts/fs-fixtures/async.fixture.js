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
  await t("readFile utf8", () => fs.readFileSync(p("w/a"), "utf8"), (cb) => fs.readFile(p("w/a"), "utf8", cb), () => fsp.readFile(p("w/a"), "utf8"));
  await t("readFile buffer", () => fs.readFileSync(p("w/a")), (cb) => fs.readFile(p("w/a"), cb), () => fsp.readFile(p("w/a")));
  await t("readFile hex", () => fs.readFileSync(p("w/a"), { encoding: "hex" }), (cb) => fs.readFile(p("w/a"), { encoding: "hex" }, cb), () => fsp.readFile(p("w/a"), { encoding: "hex" }));
  await t("readFile missing", () => fs.readFileSync(p("w/nope")), (cb) => fs.readFile(p("w/nope"), cb), () => fsp.readFile(p("w/nope")));
  await t("readFile dir", () => fs.readFileSync(p("w/sub")), (cb) => fs.readFile(p("w/sub"), cb), () => fsp.readFile(p("w/sub")));
  await t("readFile link", () => fs.readFileSync(p("link-file"), "utf8"), (cb) => fs.readFile(p("link-file"), "utf8", cb), () => fsp.readFile(p("link-file"), "utf8"));
  await t("writeFile", () => { fs.writeFileSync(p("w/n"), "new"); return fs.readFileSync(p("w/n"), "utf8"); }, (cb) => fs.writeFile(p("w/n"), "new", (e) => cb(e, e ? undefined : fs.readFileSync(p("w/n"), "utf8"))), () => fsp.writeFile(p("w/n"), "new").then(() => fs.readFileSync(p("w/n"), "utf8")), reset);
  await t("writeFile buffer", () => { fs.writeFileSync(p("w/n"), Buffer.from([1, 2, 3])); return fs.readFileSync(p("w/n")); }, (cb) => fs.writeFile(p("w/n"), Buffer.from([1, 2, 3]), (e) => cb(e, e ? undefined : fs.readFileSync(p("w/n")))), () => fsp.writeFile(p("w/n"), Buffer.from([1, 2, 3])).then(() => fs.readFileSync(p("w/n"))), reset);
  await t("writeFile wx exists", () => fs.writeFileSync(p("w/a"), "x", { flag: "wx" }), (cb) => fs.writeFile(p("w/a"), "x", { flag: "wx" }, cb), () => fsp.writeFile(p("w/a"), "x", { flag: "wx" }), reset);
  await t("writeFile missing dir", () => fs.writeFileSync(p("w/q/z"), "x"), (cb) => fs.writeFile(p("w/q/z"), "x", cb), () => fsp.writeFile(p("w/q/z"), "x"), reset);
  await t("appendFile", () => { fs.appendFileSync(p("w/a"), "++"); return fs.readFileSync(p("w/a"), "utf8"); }, (cb) => fs.appendFile(p("w/a"), "++", (e) => cb(e, e ? undefined : fs.readFileSync(p("w/a"), "utf8"))), () => fsp.appendFile(p("w/a"), "++").then(() => fs.readFileSync(p("w/a"), "utf8")), reset);
  await t("stat file", () => fs.statSync(p("w/a")), (cb) => fs.stat(p("w/a"), cb), () => fsp.stat(p("w/a")));
  await t("stat dir", () => fs.statSync(p("w/sub")), (cb) => fs.stat(p("w/sub"), cb), () => fsp.stat(p("w/sub")));
  await t("stat bigint", () => typeof fs.statSync(p("w/a"), { bigint: true }).size, (cb) => fs.stat(p("w/a"), { bigint: true }, (e, s) => cb(e, e ? undefined : typeof s.size)), () => fsp.stat(p("w/a"), { bigint: true }).then((s) => typeof s.size));
  await t("stat missing", () => fs.statSync(p("w/nope")), (cb) => fs.stat(p("w/nope"), cb), () => fsp.stat(p("w/nope")));
  await t("lstat link", () => fs.lstatSync(p("link-file")), (cb) => fs.lstat(p("link-file"), cb), () => fsp.lstat(p("link-file")));
  await t("lstat dangling", () => fs.lstatSync(p("dangling")), (cb) => fs.lstat(p("dangling"), cb), () => fsp.lstat(p("dangling")));
  await t("stat dangling", () => fs.statSync(p("dangling")), (cb) => fs.stat(p("dangling"), cb), () => fsp.stat(p("dangling")));
  await t("stat loop", () => fs.statSync(p("loop")), (cb) => fs.stat(p("loop"), cb), () => fsp.stat(p("loop")));
  await t("readdir", () => fs.readdirSync(p("w")), (cb) => fs.readdir(p("w"), cb), () => fsp.readdir(p("w")));
  await t("readdir buffer", () => fs.readdirSync(p("w"), "buffer"), (cb) => fs.readdir(p("w"), "buffer", cb), () => fsp.readdir(p("w"), "buffer"));
  await t("readdir types", () => fs.readdirSync(p("w"), { withFileTypes: true }), (cb) => fs.readdir(p("w"), { withFileTypes: true }, cb), () => fsp.readdir(p("w"), { withFileTypes: true }));
  await t("readdir recursive", () => fs.readdirSync(p("w"), { recursive: true }), (cb) => fs.readdir(p("w"), { recursive: true }, cb), () => fsp.readdir(p("w"), { recursive: true }));
  await t("readdir file", () => fs.readdirSync(p("w/a")), (cb) => fs.readdir(p("w/a"), cb), () => fsp.readdir(p("w/a")));
  await t("readdir missing", () => fs.readdirSync(p("w/nope")), (cb) => fs.readdir(p("w/nope"), cb), () => fsp.readdir(p("w/nope")));
  await t("mkdir", () => { fs.mkdirSync(p("w/m")); return fs.statSync(p("w/m")).isDirectory(); }, (cb) => fs.mkdir(p("w/m"), (e) => cb(e, e ? undefined : fs.statSync(p("w/m")).isDirectory())), () => fsp.mkdir(p("w/m")).then(() => fs.statSync(p("w/m")).isDirectory()), reset);
  await t("mkdir exists", () => fs.mkdirSync(p("w/sub")), (cb) => fs.mkdir(p("w/sub"), cb), () => fsp.mkdir(p("w/sub")), reset);
  await t("mkdir recursive", () => fs.mkdirSync(p("w/r/s/t"), { recursive: true }), (cb) => fs.mkdir(p("w/r/s/t"), { recursive: true }, cb), () => fsp.mkdir(p("w/r/s/t"), { recursive: true }), reset);
  await t("mkdir under file", () => fs.mkdirSync(p("w/a/x"), { recursive: true }), (cb) => fs.mkdir(p("w/a/x"), { recursive: true }, cb), () => fsp.mkdir(p("w/a/x"), { recursive: true }), reset);
  await t("rmdir", () => fs.rmdirSync(p("w/sub")), (cb) => fs.rmdir(p("w/sub"), cb), () => fsp.rmdir(p("w/sub")), reset);
  await t("rmdir empty", () => { fs.mkdirSync(p("w/e")); return fs.rmdirSync(p("w/e")); }, (cb) => { fs.mkdirSync(p("w/e")); fs.rmdir(p("w/e"), cb); }, () => { fs.mkdirSync(p("w/e")); return fsp.rmdir(p("w/e")); }, reset);
  await t("unlink", () => fs.unlinkSync(p("w/a")), (cb) => fs.unlink(p("w/a"), cb), () => fsp.unlink(p("w/a")), reset);
  await t("unlink missing", () => fs.unlinkSync(p("w/nope")), (cb) => fs.unlink(p("w/nope"), cb), () => fsp.unlink(p("w/nope")), reset);
  await t("rename", () => { fs.renameSync(p("w/a"), p("w/z")); return fs.readdirSync(p("w")); }, (cb) => fs.rename(p("w/a"), p("w/z"), (e) => cb(e, e ? undefined : fs.readdirSync(p("w")))), () => fsp.rename(p("w/a"), p("w/z")).then(() => fs.readdirSync(p("w"))), reset);
  await t("rename missing", () => fs.renameSync(p("w/nope"), p("w/z")), (cb) => fs.rename(p("w/nope"), p("w/z"), cb), () => fsp.rename(p("w/nope"), p("w/z")), reset);
  await t("copyFile", () => { fs.copyFileSync(p("w/a"), p("w/k")); return fs.readFileSync(p("w/k"), "utf8"); }, (cb) => fs.copyFile(p("w/a"), p("w/k"), (e) => cb(e, e ? undefined : fs.readFileSync(p("w/k"), "utf8"))), () => fsp.copyFile(p("w/a"), p("w/k")).then(() => fs.readFileSync(p("w/k"), "utf8")), reset);
  await t("copyFile excl", () => fs.copyFileSync(p("w/a"), p("w/b"), fs.constants.COPYFILE_EXCL), (cb) => fs.copyFile(p("w/a"), p("w/b"), fs.constants.COPYFILE_EXCL, cb), () => fsp.copyFile(p("w/a"), p("w/b"), fs.constants.COPYFILE_EXCL), reset);
  await t("copyFile missing", () => fs.copyFileSync(p("w/nope"), p("w/k")), (cb) => fs.copyFile(p("w/nope"), p("w/k"), cb), () => fsp.copyFile(p("w/nope"), p("w/k")), reset);
  await t("access", () => fs.accessSync(p("w/a")), (cb) => fs.access(p("w/a"), cb), () => fsp.access(p("w/a")));
  await t("access missing", () => fs.accessSync(p("w/nope")), (cb) => fs.access(p("w/nope"), cb), () => fsp.access(p("w/nope")));
  await t("realpath", () => fs.realpathSync(p("link-dir/in.txt")), (cb) => fs.realpath(p("link-dir/in.txt"), cb), () => fsp.realpath(p("link-dir/in.txt")));
  await t("realpath native", () => fs.realpathSync.native(p("link-dir")), (cb) => fs.realpath.native(p("link-dir"), cb), () => fsp.realpath(p("link-dir")));
  await t("realpath missing", () => fs.realpathSync(p("w/nope")), (cb) => fs.realpath(p("w/nope"), cb), () => fsp.realpath(p("w/nope")));
  await t("readlink", () => fs.readlinkSync(p("link-file")), (cb) => fs.readlink(p("link-file"), cb), () => fsp.readlink(p("link-file")));
  await t("readlink not link", () => fs.readlinkSync(p("w/a")), (cb) => fs.readlink(p("w/a"), cb), () => fsp.readlink(p("w/a")));
  await t("symlink", () => { fs.symlinkSync("a", p("w/ln")); return fs.readlinkSync(p("w/ln")); }, (cb) => fs.symlink("a", p("w/ln"), (e) => cb(e, e ? undefined : fs.readlinkSync(p("w/ln")))), () => fsp.symlink("a", p("w/ln")).then(() => fs.readlinkSync(p("w/ln"))), reset);
  await t("link", () => { fs.linkSync(p("w/a"), p("w/hl")); return fs.statSync(p("w/a")).nlink; }, (cb) => fs.link(p("w/a"), p("w/hl"), (e) => cb(e, e ? undefined : fs.statSync(p("w/a")).nlink)), () => fsp.link(p("w/a"), p("w/hl")).then(() => fs.statSync(p("w/a")).nlink), reset);
  await t("chmod", () => { fs.chmodSync(p("w/a"), 0o600); return (fs.statSync(p("w/a")).mode & 0o777).toString(8); }, (cb) => fs.chmod(p("w/a"), 0o600, (e) => cb(e, e ? undefined : (fs.statSync(p("w/a")).mode & 0o777).toString(8))), () => fsp.chmod(p("w/a"), 0o600).then(() => (fs.statSync(p("w/a")).mode & 0o777).toString(8)), reset);
  await t("utimes", () => { fs.utimesSync(p("w/a"), 1000, 2000); return fs.statSync(p("w/a")).mtimeMs; }, (cb) => fs.utimes(p("w/a"), 1000, 2000, (e) => cb(e, e ? undefined : fs.statSync(p("w/a")).mtimeMs)), () => fsp.utimes(p("w/a"), 1000, 2000).then(() => fs.statSync(p("w/a")).mtimeMs), reset);
  await t("truncate", () => { fs.truncateSync(p("w/a"), 2); return fs.readFileSync(p("w/a"), "utf8"); }, (cb) => fs.truncate(p("w/a"), 2, (e) => cb(e, e ? undefined : fs.readFileSync(p("w/a"), "utf8"))), () => fsp.truncate(p("w/a"), 2).then(() => fs.readFileSync(p("w/a"), "utf8")), reset);
  await t("mkdtemp", () => fs.statSync(fs.mkdtempSync(p("w/tmp-"))).isDirectory(), (cb) => fs.mkdtemp(p("w/tmp-"), (e, d) => cb(e, e ? undefined : fs.statSync(d).isDirectory())), () => fsp.mkdtemp(p("w/tmp-")).then((d) => fs.statSync(d).isDirectory()), reset);
  await t("rm file", () => fs.rmSync(p("w/a")), (cb) => fs.rm(p("w/a"), cb), () => fsp.rm(p("w/a")), reset);
  await t("rm dir", () => fs.rmSync(p("w/sub")), (cb) => fs.rm(p("w/sub"), cb), () => fsp.rm(p("w/sub")), reset);
  await t("rm recursive", () => { fs.rmSync(p("w/sub"), { recursive: true }); return fs.readdirSync(p("w")); }, (cb) => fs.rm(p("w/sub"), { recursive: true }, (e) => cb(e, e ? undefined : fs.readdirSync(p("w")))), () => fsp.rm(p("w/sub"), { recursive: true }).then(() => fs.readdirSync(p("w"))), reset);
  await t("rm force missing", () => fs.rmSync(p("w/nope"), { force: true }), (cb) => fs.rm(p("w/nope"), { force: true }, cb), () => fsp.rm(p("w/nope"), { force: true }), reset);
  await t("cp", () => { fs.cpSync(p("w/sub"), p("w/sub2"), { recursive: true }); return fs.readdirSync(p("w/sub2")); }, (cb) => fs.cp(p("w/sub"), p("w/sub2"), { recursive: true }, (e) => cb(e, e ? undefined : fs.readdirSync(p("w/sub2")))), () => fsp.cp(p("w/sub"), p("w/sub2"), { recursive: true }).then(() => fs.readdirSync(p("w/sub2"))), reset);
  await t("exists", () => fs.existsSync(p("w/a")), (cb) => fs.exists(p("w/a"), (v) => cb(null, v)), () => new Promise((r) => fs.exists(p("w/a"), r)));
  await t("exists missing", () => fs.existsSync(p("w/nope")), (cb) => fs.exists(p("w/nope"), (v) => cb(null, v)), () => new Promise((r) => fs.exists(p("w/nope"), r)));
})().catch((e) => { console.log("FIXTURE FAILED", e); process.exitCode = 1; });
