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
// #1218: cpSync (files, directories, symlinks, filters, option validation).
t("setup", () => {
  fs.mkdirSync(p("src/sub"), { recursive: true });
  fs.writeFileSync(p("src/a.txt"), "a");
  fs.writeFileSync(p("src/sub/b.txt"), "b");
  fs.chmodSync(p("src/a.txt"), 0o640);
  fs.symlinkSync("a.txt", p("src/rel-link"));
  fs.symlinkSync("sub", p("src/dir-link"));
  fs.mkdirSync(p("empty"));
  fs.writeFileSync(p("file"), "f");
});
const tree = (dir) => fs.readdirSync(p(dir), { recursive: true }).sort();
t("cp file", () => { fs.cpSync(p("file"), p("f2")); return fs.readFileSync(p("f2"), "utf8"); });
t("cp file overwrite", () => { fs.writeFileSync(p("f3"), "old"); fs.cpSync(p("file"), p("f3")); return fs.readFileSync(p("f3"), "utf8"); });
t("cp file no force", () => { fs.writeFileSync(p("f4"), "old"); fs.cpSync(p("file"), p("f4"), { force: false }); return fs.readFileSync(p("f4"), "utf8"); });
t("cp file errorOnExist", () => fs.cpSync(p("file"), p("f4"), { force: false, errorOnExist: true }));
t("cp file mode", () => { fs.cpSync(p("src/a.txt"), p("f5")); return (fs.statSync(p("f5")).mode & 0o777).toString(8); });
t("cp file new parent", () => { fs.cpSync(p("file"), p("newparent/x/f6")); return fs.readFileSync(p("newparent/x/f6"), "utf8"); });
t("cp file mode EXCL", () => { fs.cpSync(p("file"), p("f7"), { mode: fs.constants.COPYFILE_EXCL }); return fs.cpSync(p("file"), p("f7"), { mode: fs.constants.COPYFILE_EXCL }); });
t("cp file preserveTimestamps", () => { fs.utimesSync(p("file"), 100, 200); fs.cpSync(p("file"), p("f8"), { preserveTimestamps: true }); const s = fs.statSync(p("f8")); return [s.atimeMs, s.mtimeMs]; });
t("cp file overwrite preserveTimestamps", () => { fs.cpSync(p("file"), p("f8"), { preserveTimestamps: true }); return fs.statSync(p("f8")).mtimeMs; });
t("cp missing", () => fs.cpSync(p("nope"), p("x")));
t("cp missing dereference", () => fs.cpSync(p("nope"), p("x"), { dereference: true }));
t("cp dangling", () => fs.cpSync(p("dangling"), p("x")));
t("cp dir not recursive", () => fs.cpSync(p("src"), p("d0")));
t("cp dir", () => { fs.cpSync(p("src"), p("d1"), { recursive: true }); return tree("d1"); });
t("cp dir content", () => [fs.readFileSync(p("d1/a.txt"), "utf8"), fs.readFileSync(p("d1/sub/b.txt"), "utf8"), (fs.statSync(p("d1/a.txt")).mode & 0o777).toString(8)]);
t("cp dir symlinks", () => [fs.lstatSync(p("d1/rel-link")).isSymbolicLink(), norm(fs.readlinkSync(p("d1/rel-link"))), norm(fs.readlinkSync(p("d1/dir-link")))]);
t("cp dir again", () => { fs.cpSync(p("src"), p("d1"), { recursive: true }); return tree("d1"); });
t("cp dir force false", () => { fs.writeFileSync(p("d1/a.txt"), "changed"); fs.cpSync(p("src"), p("d1"), { recursive: true, force: false }); return fs.readFileSync(p("d1/a.txt"), "utf8"); });
t("cp dir errorOnExist", () => { fs.mkdirSync(p("one")); fs.writeFileSync(p("one/only.txt"), "1"); fs.cpSync(p("one"), p("one2"), { recursive: true }); return fs.cpSync(p("one"), p("one2"), { recursive: true, force: false, errorOnExist: true }); });
t("cp dir overwrite", () => { fs.cpSync(p("src"), p("d1"), { recursive: true }); return fs.readFileSync(p("d1/a.txt"), "utf8"); });
t("cp dir verbatimSymlinks", () => { fs.cpSync(p("src"), p("d2"), { recursive: true, verbatimSymlinks: true }); return [fs.readlinkSync(p("d2/rel-link")), fs.readlinkSync(p("d2/dir-link"))]; });
t("cp dir empty", () => { fs.cpSync(p("empty"), p("d3"), { recursive: true }); return fs.readdirSync(p("d3")); });
t("cp dir filter", () => { fs.cpSync(p("src"), p("d4"), { recursive: true, filter: (s) => !s.endsWith("b.txt") }); return tree("d4"); });
t("cp dir filter args", () => { const seen = []; fs.cpSync(p("src"), p("d5"), { recursive: true, filter: (s, d) => { seen.push(norm(s) + ">" + norm(d)); return true; } }); return seen.sort(); });
t("cp dir filter false root", () => { fs.cpSync(p("src"), p("d6"), { recursive: true, filter: () => false }); return fs.existsSync(p("d6")); });
t("cp dir filter promise", () => fs.cpSync(p("src"), p("d7"), { recursive: true, filter: async () => true }));
t("cp dir filter non-function", () => fs.cpSync(p("src"), p("d7"), { filter: 1 }));
t("cp link", () => { fs.cpSync(p("src/rel-link"), p("l1")); return [fs.lstatSync(p("l1")).isSymbolicLink(), norm(fs.readlinkSync(p("l1")))]; });
t("cp link verbatim", () => { fs.cpSync(p("src/rel-link"), p("l2"), { verbatimSymlinks: true }); return fs.readlinkSync(p("l2")); });
t("cp link overwrite", () => { fs.cpSync(p("src/rel-link"), p("l1")); return norm(fs.readlinkSync(p("l1"))); });
t("cp same", () => fs.cpSync(p("file"), p("file")));
t("cp same dir", () => fs.cpSync(p("src"), p("src"), { recursive: true }));
t("cp same via link", () => fs.cpSync(p("target.txt"), p("link-file")));
t("cp into self", () => fs.cpSync(p("src"), p("src/sub/inner"), { recursive: true }));
t("cp dir to file", () => fs.cpSync(p("src"), p("file"), { recursive: true }));
t("cp file to dir", () => fs.cpSync(p("file"), p("empty")));
t("cp file to dir recursive", () => fs.cpSync(p("file"), p("empty"), { recursive: true }));
t("cp dir to subdir relative", () => fs.cpSync(p("src/sub"), p("src"), { recursive: true }));
t("cp options bad", () => fs.cpSync(p("file"), p("o1"), 1));
t("cp options bad recursive", () => fs.cpSync(p("file"), p("o1"), { recursive: 1 }));
t("cp options bad force", () => fs.cpSync(p("file"), p("o1"), { force: "no" }));
t("cp options incompatible", () => fs.cpSync(p("file"), p("o1"), { dereference: true, verbatimSymlinks: true }));
t("cp options mode", () => fs.cpSync(p("file"), p("o1"), { mode: 9 }));
t("cp bad src", () => fs.cpSync(1, p("o1")));
t("cp bad dest", () => fs.cpSync(p("file"), 1));
t("cp src null byte", () => fs.cpSync("a\0b", p("o1")));
t("cp symlink over directory", () => { fs.mkdirSync(p("sd1/rel-link"), { recursive: true }); return fs.cpSync(p("src"), p("sd1"), { recursive: true }); });
t("cp symlink over file", () => { fs.mkdirSync(p("sd2")); fs.writeFileSync(p("sd2/rel-link"), "x"); return fs.cpSync(p("src"), p("sd2"), { recursive: true }); });
t("cp filter throws keeps mode", () => {
  fs.mkdirSync(p("ro"));
  fs.writeFileSync(p("ro/f"), "x");
  fs.chmodSync(p("ro"), 0o555);
  try {
    fs.cpSync(p("ro"), p("ro2"), { recursive: true, filter: () => { throw new Error("stop"); } });
  } catch (e) {
    return [e.message, (fs.statSync(p("ro2")).mode & 0o777).toString(8)];
  } finally {
    fs.chmodSync(p("ro"), 0o755);
  }
});
