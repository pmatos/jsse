const fs = typeof require === "function" ? require("fs") : __node_builtins.fs;
const D = process.env.FS_ORACLE_DIR;
if (!D) throw new Error("FS_ORACLE_DIR is not set");
const norm = (s) => String(s).split(D).join("<D>");
function show(v) {
  if (v === undefined) return "undefined";
  if (Buffer.isBuffer(v)) return "buf:" + v.toString("hex");
  if (typeof v === "string") return JSON.stringify(v);
  if (Array.isArray(v)) return "[" + v.map(show).join(",") + "]";
  return String(typeof v === "object" ? JSON.stringify(v) : v);
}
function t(label, fn) {
  try {
    console.log(label, "=>", norm(show(fn())));
  } catch (e) {
    console.log(label, "!!", e.name, e.code, e.errno, e.syscall, norm(e.path), norm(e.dest), norm(e.message));
  }
}
const p = (name) => D + "/" + name;
// #1217: stat, readdir, mkdir, rmdir, unlink, rename, access, realpath.
t("mkdir", () => fs.mkdirSync(p("sub")));
t("mkdir again", () => fs.mkdirSync(p("sub")));
t("mkdir missing parent", () => fs.mkdirSync(p("x/y")));
t("mkdir mode", () => { fs.mkdirSync(p("sub/m"), 0o700); return (fs.statSync(p("sub/m")).mode & 0o777).toString(8); });
t("mkdir {mode}", () => { fs.mkdirSync(p("sub/m2"), { mode: 0o750 }); return (fs.statSync(p("sub/m2")).mode & 0o777).toString(8); });
t("write files", () => { fs.writeFileSync(p("sub/b"), "bb"); fs.writeFileSync(p("sub/a"), "a"); fs.writeFileSync(p("sub/B"), "B"); fs.writeFileSync(p("sub/é"), "e"); });
t("readdir", () => fs.readdirSync(p("sub")));
t("readdir buffer", () => fs.readdirSync(p("sub"), "buffer"));
t("readdir {encoding:buffer}", () => fs.readdirSync(p("sub"), { encoding: "buffer" }).length);
t("readdir file", () => fs.readdirSync(p("sub/a")));
t("readdir missing", () => fs.readdirSync(p("nope")));
t("stat file", () => {
  const s = fs.statSync(p("sub/b"));
  return [s.isFile(), s.isDirectory(), s.isSymbolicLink(), s.isFIFO(), s.isSocket(), s.isBlockDevice(), s.isCharacterDevice(), s.size, s.mode.toString(8), s.nlink, typeof s.ino, typeof s.dev, s.atime instanceof Date, s.mtimeMs > 1e12, Math.abs(s.mtime.getTime() - s.mtimeMs) < 1, s.birthtimeMs > 0, s.blksize > 0, s.blocks >= 0];
});
t("stat dir", () => { const s = fs.statSync(p("sub")); return [s.isDirectory(), s.isFile(), s.mode.toString(8).slice(0, 2)]; });
t("stat missing", () => fs.statSync(p("nope")));
t("stat missing noThrow", () => fs.statSync(p("nope"), { throwIfNoEntry: false }));
t("stat Buffer path", () => fs.statSync(Buffer.from(p("sub/a"))).size);
// The oracle script pre-creates: target.txt, real-dir/in.txt and the symlinks
// link-file -> target.txt, link-dir -> real-dir, dangling -> missing,
// loop -> loop.
t("lstat link", () => { const s = fs.lstatSync(p("link-file")); return [s.isSymbolicLink(), s.isFile(), s.size]; });
t("stat link", () => { const s = fs.statSync(p("link-file")); return [s.isSymbolicLink(), s.isFile(), s.size]; });
t("stat dangling", () => fs.statSync(p("dangling")));
t("lstat dangling", () => fs.lstatSync(p("dangling")).isSymbolicLink());
t("stat loop", () => fs.statSync(p("loop")));
t("read via link", () => fs.readFileSync(p("link-file"), "utf8"));
t("readdir via link", () => fs.readdirSync(p("link-dir")));
t("readdir root", () => fs.readdirSync(D));
t("realpath link dir", () => fs.realpathSync(p("link-dir/in.txt")));
t("realpath link file", () => fs.realpathSync(p("link-file")));
t("realpath dangling", () => fs.realpathSync(p("dangling")));
t("realpath loop", () => fs.realpathSync(p("loop")));
t("realpath native link", () => fs.realpathSync.native(p("link-dir")));
t("realpath up link", () => fs.realpathSync(p("real-dir/up")));
t("access ok", () => fs.accessSync(p("sub/a")));
t("access R_OK", () => fs.accessSync(p("sub/a"), fs.constants.R_OK));
t("access missing", () => fs.accessSync(p("nope")));
t("access missing W_OK", () => fs.accessSync(p("nope"), fs.constants.W_OK));
t("access bad mode", () => fs.accessSync(p("sub/a"), 8));
t("access bad mode type", () => fs.accessSync(p("sub/a"), "r"));
t("rename", () => { fs.renameSync(p("sub/a"), p("sub/a2")); return [fs.existsSync(p("sub/a")), fs.existsSync(p("sub/a2"))]; });
t("rename missing", () => fs.renameSync(p("sub/zz"), p("sub/yy")));
t("rename over", () => { fs.renameSync(p("sub/a2"), p("sub/b")); return fs.readFileSync(p("sub/b"), "utf8"); });
t("unlink", () => { fs.unlinkSync(p("sub/b")); return fs.existsSync(p("sub/b")); });
t("unlink missing", () => fs.unlinkSync(p("sub/b")));
t("unlink dir", () => fs.unlinkSync(p("sub/m")));
t("rmdir nonempty", () => fs.rmdirSync(p("sub")));
t("rmdir", () => { fs.rmdirSync(p("sub/m")); return fs.existsSync(p("sub/m")); });
t("rmdir file", () => fs.rmdirSync(p("sub/B")));
t("rmdir missing", () => fs.rmdirSync(p("sub/m")));
t("realpath", () => fs.realpathSync(p("sub/../sub/./m2")));
t("realpath native", () => fs.realpathSync.native(p("sub/../sub/./m2")));
t("realpath missing", () => fs.realpathSync(p("nope/x")));
t("realpath buffer enc", () => [Buffer.isBuffer(fs.realpathSync(p("sub"), "buffer")), fs.realpathSync(p("sub"), "buffer").toString()]);
t("lstat", () => { const s = fs.lstatSync(p("sub")); return [s.isDirectory(), s.isSymbolicLink()]; });
t("lstat missing", () => fs.lstatSync(p("nope")));
t("fstat", () => { const fd = fs.openSync(p("sub/B")); const s = fs.fstatSync(fd); fs.closeSync(fd); return [s.size, s.isFile()]; });
t("readdir hex", () => fs.readdirSync(p("sub"), "hex"));
t("realpath native buffer", () => fs.realpathSync.native(p("sub"), "buffer").toString());
