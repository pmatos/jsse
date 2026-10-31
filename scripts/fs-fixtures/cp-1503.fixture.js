// Issue #1503: the asynchronous fs.cp / fs.promises.cp (Node's promise-based
// copy, not cpSync): files, directories, symlinks, option combinations, sync
// and async filters, the error shapes of the async walk and the arguments of the
// callback, each cross-checked against Node byte for byte. Every operation is
// awaited and the trees are listed sorted, so the output does not depend on the
// order the OS lists a directory or on pool completion order.
const fs = require("fs");
const fsp = fs.promises;
const D = process.env.FS_ORACLE_DIR;
if (!D) throw new Error("FS_ORACLE_DIR is not set");
const norm = (s) => String(s).split(D).join("<D>").split(process.cwd()).join("<CWD>");
const p = (name) => D + "/" + name;
const fail = (e) => ("!! " + e.name + " " + e.code + " " + e.errno + " " + e.syscall + " " + norm(e.path) + " " + norm(e.dest) + " " + norm(e.message) + " [" + Object.keys(e).join() + "]");
const tree = (dir) => {
  try {
    return fs.readdirSync(p(dir), { recursive: true }).sort().map((n) => {
      const s = fs.lstatSync(p(dir + "/" + n));
      return n + (s.isSymbolicLink() ? "@" + norm(fs.readlinkSync(p(dir + "/" + n))) : s.isDirectory() ? "/" : ":" + fs.readFileSync(p(dir + "/" + n), "utf8"));
    }).join(" ");
  } catch (e) {
    return "no tree " + e.code;
  }
};
const reset = () => {
  fs.rmSync(p("w"), { recursive: true, force: true });
  fs.mkdirSync(p("w/src/sub/deep"), { recursive: true });
  fs.mkdirSync(p("w/empty"));
  fs.writeFileSync(p("w/file"), "f");
  fs.writeFileSync(p("w/src/a.txt"), "a");
  fs.writeFileSync(p("w/src/sub/b.txt"), "b");
  fs.writeFileSync(p("w/src/sub/deep/c.txt"), "c");
  fs.chmodSync(p("w/src/a.txt"), 0o640);
  fs.utimesSync(p("w/src/a.txt"), 1000, 2000);
  fs.chmodSync(p("w/src/sub"), 0o750);
  fs.symlinkSync("a.txt", p("w/src/rel-link"));
  fs.symlinkSync("sub", p("w/src/dir-link"));
  fs.symlinkSync("missing", p("w/src/dangling"));
  fs.symlinkSync(p("w/file"), p("w/src/abs-link"));
};
const viaCallback = (fn) => new Promise((resolve) => {
  try {
    fn(function (e, ...v) { resolve((e ? fail(e) : "ok") + " args=" + arguments.length + " " + v.map(String).join()); });
  } catch (e) {
    resolve("threw " + fail(e));
  }
});
const viaPromise = (fn) => {
  try {
    return fn().then((v) => "resolved " + String(v), (e) => fail(e));
  } catch (e) {
    return Promise.resolve("threw " + fail(e));
  }
};
const cases = [];
const c = (label, src, dest, options, after) => cases.push({ label, src, dest, options, after });
const ls = (dir) => () => tree(dir);

c("file", "w/file", "w/f2", undefined, ls("w"));
c("file overwrite", "w/file", "w/src/a.txt", undefined, () => fs.readFileSync(p("w/src/a.txt"), "utf8"));
c("file no force", "w/file", "w/src/a.txt", { force: false }, () => fs.readFileSync(p("w/src/a.txt"), "utf8"));
c("file errorOnExist", "w/file", "w/src/a.txt", { force: false, errorOnExist: true });
c("file errorOnExist force", "w/file", "w/src/a.txt", { errorOnExist: true }, () => fs.readFileSync(p("w/src/a.txt"), "utf8"));
c("file mode", "w/src/a.txt", "w/f2", undefined, () => (fs.statSync(p("w/f2")).mode & 0o777).toString(8));
c("file EXCL", "w/file", "w/src/a.txt", { mode: fs.constants.COPYFILE_EXCL });
c("file new parent", "w/file", "w/n/m/f2", undefined, ls("w/n"));
c("file preserveTimestamps", "w/src/a.txt", "w/f2", { preserveTimestamps: true }, () => { const s = fs.statSync(p("w/f2")); return [s.mtimeMs, fs.statSync(p("w/src/a.txt")).mtimeMs, (s.mode & 0o777).toString(8)].join(); });
c("file read-only preserveTimestamps", "w/src/a.txt", "w/f2", { preserveTimestamps: true }, () => { fs.chmodSync(p("w/src/a.txt"), 0o440); return 0; });
c("missing src", "w/nope", "w/x");
c("missing src dereference", "w/nope", "w/x", { dereference: true });
c("dangling", "w/src/dangling", "w/x");
c("dangling dereference", "w/src/dangling", "w/x", { dereference: true });
c("dir not recursive", "w/src", "w/d1", undefined, ls("w"));
c("dir", "w/src", "w/d1", { recursive: true }, ls("w/d1"));
c("dir modes", "w/src", "w/d1", { recursive: true }, () => ["d1", "d1/sub", "d1/a.txt"].map((n) => (fs.statSync(p("w/" + n)).mode & 0o777).toString(8)).join());
c("dir onto existing", "w/src", "w/empty", { recursive: true }, ls("w/empty"));
c("dir force false", "w/src", "w/empty", { recursive: true, force: false }, ls("w/empty"));
c("dir errorOnExist", "w/src", "w/empty", { recursive: true, force: false, errorOnExist: true }, ls("w/empty"));
c("dir dereference", "w/src", "w/d1", { recursive: true, dereference: true });
c("dir dereference no dangling", "w/src", "w/d1", { recursive: true, dereference: true, filter: (s) => !s.endsWith("dangling") }, ls("w/d1"));
c("dir verbatimSymlinks", "w/src", "w/d1", { recursive: true, verbatimSymlinks: true }, ls("w/d1"));
c("dir preserveTimestamps", "w/src", "w/d1", { recursive: true, preserveTimestamps: true }, ls("w/d1"));
c("dir empty", "w/empty", "w/d2", { recursive: true }, ls("w"));
c("dir filter", "w/src", "w/d1", { recursive: true, filter: (s) => !s.endsWith("b.txt") }, ls("w/d1"));
c("dir filter dir", "w/src", "w/d1", { recursive: true, filter: (s) => !s.endsWith("deep") }, ls("w/d1"));
c("dir filter root false", "w/src", "w/d1", { recursive: true, filter: () => false }, ls("w"));
c("dir filter falsy", "w/src", "w/d1", { recursive: true, filter: (s) => (s.endsWith("a.txt") ? 0 : 1) }, ls("w/d1"));
c("dir filter async", "w/src", "w/d1", { recursive: true, filter: async (s) => !s.endsWith("c.txt") }, ls("w/d1"));
c("dir filter async delayed", "w/src", "w/d1", { recursive: true, filter: (s) => new Promise((r) => setTimeout(() => r(!s.includes("sub")), 5)) }, ls("w/d1"));
c("dir filter async false root", "w/src", "w/d1", { recursive: true, filter: async () => false }, ls("w"));
c("dir filter rejects", "w/src", "w/d1", { recursive: true, filter: async (s) => { if (s.endsWith("b.txt")) throw new RangeError("no b"); return true; } });
c("dir filter throws", "w/src", "w/d1", { recursive: true, filter: (s) => { if (s.endsWith("b.txt")) throw new RangeError("no b"); return true; } });
c("dir filter rejects falsy", "w/src", "w/d1", { recursive: true, filter: () => Promise.reject(0) });
c("file filter async", "w/file", "w/f2", { filter: async () => true }, ls("w"));
c("same", "w/file", "w/file");
c("same dir", "w/src", "w/src", { recursive: true });
c("same via link", "w/file", "w/src/abs-link");
c("into self", "w/src", "w/src/sub/inner", { recursive: true }, ls("w/src"));
c("into self deep", "w/src", "w/src/sub/deep/x/y", { recursive: true });
c("dir to file", "w/src", "w/file", { recursive: true });
c("file to dir", "w/file", "w/empty");
c("file to dir recursive", "w/file", "w/empty", { recursive: true });
c("dir to subdir relative", "w/src/sub", "w/src", { recursive: true });
c("link", "w/src/rel-link", "w/l1", undefined, ls("w"));
c("link verbatim", "w/src/rel-link", "w/l1", { verbatimSymlinks: true }, ls("w"));
c("link dereference", "w/src/rel-link", "w/l1", { dereference: true }, ls("w"));
c("link dir", "w/src/dir-link", "w/l1", undefined, ls("w"));
c("link over link", "w/src/rel-link", "w/src/abs-link", undefined, () => norm(fs.readlinkSync(p("w/src/abs-link"))));
c("link over file", "w/src/rel-link", "w/file");
c("link over dir", "w/src/rel-link", "w/empty");
c("link into subdir of self", "w/src/dir-link", "w/src/sub", undefined);
c("dest parent is file", "w/file", "w/file/x");
c("src is a dev node", "/dev/null", "w/devnull", undefined, ls("w"));
c("src is a dev node force", "/dev/null", "w/file", undefined, () => fs.statSync(p("w/file")).isCharacterDevice());

(async () => {
  for (const { label, src, dest, options, after } of cases) {
    for (const [form, run] of [
      ["cb", (s, d, o) => viaCallback((cb) => (o === undefined ? fs.cp(s, d, cb) : fs.cp(s, d, o, cb)))],
      ["promise", (s, d, o) => viaPromise(() => fsp.cp(s, d, o))],
    ]) {
      reset();
      const abs = (x) => (x.startsWith("/") ? x : p(x));
      const result = await run(abs(src), abs(dest), options);
      let extra = "";
      if (after) {
        try { extra = " | " + norm(after()); } catch (e) { extra = " | after " + fail(e); }
      }
      console.log(form, label, "=>", norm(result) + extra);
    }
  }

  // The filter sees (src, dest) of every entry, root first.
  reset();
  const seen = [];
  await fsp.cp(p("w/src"), p("w/d1"), { recursive: true, filter: async (s, d) => { seen.push(norm(s) + ">" + norm(d)); return true; } });
  console.log("filter args", seen.sort().join(" "));
  console.log("filter order root first", seen[0] === norm(p("w/src")) + ">" + norm(p("w/d1")));

  // Validation throws (callback form) or rejects (promise form).
  reset();
  const noop = () => {};
  const thrown = {
    "no callback": () => fs.cp(p("w/file"), p("w/x")),
    "callback not a function": () => fs.cp(p("w/file"), p("w/x"), {}, 5),
    "options as callback": () => fs.cp(p("w/file"), p("w/x"), noop),
    "bad src": () => fs.cp(1, p("w/x"), noop),
    "bad dest": () => fs.cp(p("w/file"), 1, noop),
    "bad options": () => fs.cp(p("w/file"), p("w/x"), 1, noop),
    "bad force": () => fs.cp(p("w/file"), p("w/x"), { force: 1 }, noop),
    "bad mode": () => fs.cp(p("w/file"), p("w/x"), { mode: 9 }, noop),
    "bad filter": () => fs.cp(p("w/file"), p("w/x"), { filter: 1 }, noop),
    "incompatible": () => fs.cp(p("w/file"), p("w/x"), { dereference: true, verbatimSymlinks: true }, noop),
    "null byte": () => fs.cp("a\0b", p("w/x"), noop),
  };
  for (const [label, fn] of Object.entries(thrown)) {
    try { fn(); console.log("throws", label, "none"); } catch (e) { console.log("throws", label, fail(e)); }
  }
  console.log("nothing was copied", !fs.existsSync(p("w/x")));
  const rejected = {
    "bad src": () => fsp.cp(1, p("w/x")),
    "bad options": () => fsp.cp(p("w/file"), p("w/x"), 1),
    "bad filter": () => fsp.cp(p("w/file"), p("w/x"), { filter: "no" }),
  };
  for (const [label, fn] of Object.entries(rejected)) {
    const promise = fn();
    console.log("rejects", label, promise instanceof Promise, await promise.then(() => "resolved", fail));
  }

  // util.promisify(fs.cp) and the sync form are unaffected by the async walk.
  reset();
  await require("util").promisify(fs.cp)(p("w/src"), p("w/d9"), { recursive: true });
  console.log("promisify", tree("w/d9"));
  fs.cpSync(p("w/src"), p("w/d8"), { recursive: true });
  console.log("cpSync", tree("w/d8") === tree("w/d9"));
})();
