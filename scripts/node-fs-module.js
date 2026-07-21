// CommonJS `fs` selector for bundled library dependencies.
//
// css-tree's test suite reads its own fixture tree (JSON test cases, a couple
// of top-level .css/.ast files, and package.json) with readFileSync/
// readdirSync/statSync().isDirectory() — confirmed the only three fs
// operations it uses. Deliberately unconditional on both engines (unlike the
// buffer/util/assert selectors): the runner executes the generated bundle
// from an arbitrary cwd, not the library's clone directory, so real fs calls
// on the Node reference run would fail to resolve the suite's own relative
// paths (`./fixtures/...`). There is also no independent-oracle value to
// give up here — the manifest below is a verbatim capture of the same files
// real fs would have returned, taken at generation time, so a bug in this
// module's dictionary lookup fails loudly (a thrown ENOENT) on both engines
// alike rather than silently diverging between them. JSSE and Node both get
// this same implementation, reading the manifest a generated entry installs
// at globalThis.__VFS_MANIFEST__ before any fixture-loading module runs (ESM
// module evaluation order guarantees the manifest module, imported first,
// finishes before later imports start).

function normalize(path) {
  var segments = String(path).split("/");
  var out = [];
  for (var i = 0; i < segments.length; i++) {
    var segment = segments[i];
    if (segment === "" || segment === ".") continue;
    out.push(segment);
  }
  return out.join("/");
}

function manifest() {
  var vfs = globalThis.__VFS_MANIFEST__;
  if (!vfs) {
    throw new Error(
      "node-fs-module.js: globalThis.__VFS_MANIFEST__ is not set " +
        "(the generated entry must import its manifest module first)"
    );
  }
  return vfs;
}

function readFileSync(path, encoding) {
  var key = normalize(path);
  var content = manifest().files[key];
  if (content === undefined) {
    var error = new Error("ENOENT: no such file or directory, open '" + path + "'");
    error.code = "ENOENT";
    throw error;
  }
  return content;
}

function readdirSync(path) {
  var key = normalize(path);
  var entries = manifest().dirs[key];
  if (entries === undefined) {
    var error = new Error("ENOENT: no such file or directory, scandir '" + path + "'");
    error.code = "ENOENT";
    throw error;
  }
  return entries.slice();
}

function statSync(path) {
  var key = normalize(path);
  var vfs = manifest();
  var isDir = Object.prototype.hasOwnProperty.call(vfs.dirs, key);
  var isFile = Object.prototype.hasOwnProperty.call(vfs.files, key);
  if (!isDir && !isFile) {
    var error = new Error("ENOENT: no such file or directory, stat '" + path + "'");
    error.code = "ENOENT";
    throw error;
  }
  return {
    isDirectory: function () {
      return isDir;
    },
    isFile: function () {
      return isFile;
    },
  };
}

module.exports = {
  readFileSync: readFileSync,
  readdirSync: readdirSync,
  statSync: statSync,
};
