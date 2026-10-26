const fs = require("fs");
const D = process.env.FS_ORACLE_DIR;
if (!D) throw new Error("FS_ORACLE_DIR is not set");
const norm = (s) => String(s).split(D).join("<D>");
function t(label, fn) {
  try {
    fn();
    console.log(label, "=> no error");
  } catch (e) {
    console.log(label, "!!", e.constructor.name, e.code, e.errno, e.syscall, norm(e.path), norm(e.message));
  }
}
// #1217: invalid arguments throw Node's TypeErrors/RangeErrors with its codes.
const p = (name) => D + "/" + name;
fs.writeFileSync(p("f"), "0123456789");
for (const [name, bad] of [["number", 1.5], ["null", null], ["undefined", undefined], ["object", {}], ["array", []], ["boolean", true], ["function", function foo() {}]]) {
  t("readFileSync path " + name, () => fs.readFileSync(bad));
  t("writeFileSync path " + name, () => fs.writeFileSync(bad, "x"));
  t("statSync path " + name, () => fs.statSync(bad));
  t("readdirSync path " + name, () => fs.readdirSync(bad));
}
t("statSync path symbol direct", () => fs.statSync(Symbol("s")));
t("path null byte", () => fs.readFileSync("a\0b"));
t("path null byte buffer", () => fs.readFileSync(Buffer.from("a\0b")));
t("rename null byte", () => fs.renameSync(p("f"), "a\0b"));
t("rename bad old", () => fs.renameSync(1, p("g")));
t("rename bad new", () => fs.renameSync(p("f"), 1));
t("fd float", () => fs.closeSync(1.5));
t("fd string", () => fs.closeSync("1"));
t("fd negative", () => fs.closeSync(-1));
t("fd huge", () => fs.closeSync(2 ** 31));
t("fd EBADF", () => fs.closeSync(9999));
t("read EBADF", () => fs.readSync(9999, Buffer.alloc(1), 0, 1, null));
t("write EBADF", () => fs.writeSync(9999, "x"));
t("fstat EBADF", () => fs.fstatSync(9999));
t("write read-only fd", () => { const fd = fs.openSync(p("f"), "r"); try { fs.writeSync(fd, "x"); } finally { fs.closeSync(fd); } });
t("read write-only fd", () => { const fd = fs.openSync(p("f"), "a"); try { fs.readSync(fd, Buffer.alloc(1), 0, 1, null); } finally { fs.closeSync(fd); } });
t("encoding bad", () => fs.readFileSync(p("f"), "nope"));
t("encoding bad opt", () => fs.readFileSync(p("f"), { encoding: "nope" }));
t("options number", () => fs.readFileSync(p("f"), 1));
t("flag bad", () => fs.readFileSync(p("f"), { flag: "zz" }));
t("flag bad open", () => fs.openSync(p("f"), "zz"));
t("mode bad", () => fs.openSync(p("f"), "r", "xyz"));
t("mode float", () => fs.openSync(p("f"), "r", 1.5));
t("mode negative", () => fs.openSync(p("f"), "r", -1));
t("data number", () => fs.writeFileSync(p("g"), 1));
t("data object", () => fs.writeFileSync(p("g"), {}));
t("data undefined", () => fs.writeFileSync(p("g")));
t("append data null", () => fs.appendFileSync(p("g"), null));
t("readSync buffer string", () => fs.readSync(3, "x", 0, 1, null));
t("readSync offset float", () => fs.readSync(3, Buffer.alloc(4), 1.5, 1, null));
t("readSync offset neg", () => fs.readSync(3, Buffer.alloc(4), -1, 1, null));
t("readSync length big", () => fs.readSync(3, Buffer.alloc(4), 0, 5, null));
t("readSync position str", () => fs.readSync(3, Buffer.alloc(4), 0, 1, "x"));
t("readSync empty buffer", () => fs.readSync(3, Buffer.alloc(0), 0, 1, null));
t("readSync options string", () => fs.readSync(3, Buffer.alloc(4), "x"));
t("writeSync buffer offset big", () => fs.writeSync(1, Buffer.alloc(2), 3));
t("writeSync length big", () => fs.writeSync(1, Buffer.alloc(2), 0, 3));
t("writeSync number", () => fs.writeSync(1, 5));
t("writeSync enc unknown", () => { const fd = fs.openSync(p("f"), "r+"); try { return fs.writeSync(fd, "ab", null, "nope"); } finally { fs.closeSync(fd); } });
t("writeSync enc hex odd", () => { const fd = fs.openSync(p("f"), "r+"); try { fs.writeSync(fd, "abc", null, "hex"); } finally { fs.closeSync(fd); } });
t("accessSync mode", () => fs.accessSync(p("f"), 1.5));
t("mkdirSync mode bad", () => fs.mkdirSync(p("d"), "xyz"));
t("open ENOENT", () => fs.openSync(p("nope/x"), "r"));
t("open EEXIST", () => fs.openSync(p("f"), "wx"));
t("open ENOTDIR", () => fs.openSync(p("f/x"), "r"));
t("open EISDIR", () => fs.openSync(D, "w"));
t("read EISDIR", () => fs.readFileSync(D));
t("readdir ENOTDIR", () => fs.readdirSync(p("f")));
t("EACCES", () => { fs.writeFileSync(p("ro"), "x", { mode: 0o000 }); if (process.getuid && process.getuid() === 0) throw Object.assign(new Error("EACCES: skipped as root"), { code: "EACCES" }); fs.readFileSync(p("ro")); });
t("ENAMETOOLONG", () => fs.readFileSync(p("a".repeat(300))));
