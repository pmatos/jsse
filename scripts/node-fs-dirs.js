// Node host-compat `fs` directories (issue #1218): readdir with
// `withFileTypes`/`recursive`, mkdir `recursive`, mkdtemp, opendir/Dir and rm.
// Runs after node-fs-meta.js and parks its exports on
// `__node_fs_support.dirs` (and the path helpers on `.path`).

(function () {
  "use strict";

  var S = globalThis.__node_fs_support;
  if (!S) return;
  var host = S.host;
  var meta = S.meta;
  var stats = S.stats;
  var nodeError = S.nodeError;
  var getOptions = S.getOptions;
  var getValidatedPath = S.getValidatedPath;
  var validateBoolean = S.validateBoolean;
  var validateInteger = S.validateInteger;

  // ---- posix path helpers (the `path` module is not loadable this early) ---

  function normalize(path) {
    var absolute = path.charAt(0) === "/";
    var out = [];
    path.split("/").forEach(function (seg) {
      if (seg === "" || seg === ".") return;
      if (seg === ".." && out.length > 0 && out[out.length - 1] !== "..") out.pop();
      else if (seg !== ".." || !absolute) out.push(seg);
    });
    var joined = out.join("/");
    if (absolute) return "/" + joined;
    return joined === "" ? "." : joined;
  }

  function join(a, b) {
    if (typeof b !== "string") throw S.invalidArgType("path", "of type string", b);
    return normalize(a + "/" + b);
  }

  function isAbsolute(path) {
    return path.charAt(0) === "/";
  }

  function dirname(path) {
    var end = path.replace(/\/+$/, "");
    var i = end.lastIndexOf("/");
    if (i === -1) return ".";
    return i === 0 ? "/" : end.slice(0, i).replace(/\/+$/, "") || "/";
  }

  function resolve(from, to) {
    if (isAbsolute(to)) return normalize(to);
    return normalize((isAbsolute(from) ? from : process.cwd() + "/" + from) + "/" + to);
  }

  // ---- readdir -------------------------------------------------------------

  var DIRENT_UNKNOWN = 0;

  function getDirent(path, name, type) {
    if (type === DIRENT_UNKNOWN) {
      var st = meta.lstatSync(join(path, String(name)));
      return stats.direntFromStats(name, st, path);
    }
    return new stats.Dirent(name, type, path);
  }

  function listing(path, options) {
    var raw = !meta.plainEncoding(options.encoding);
    var result = host.readdir_types(path, raw, false);
    if (raw) {
      result[0] = result[0].map(function (n) {
        return meta.encodeName(n, options.encoding);
      });
    }
    return result;
  }

  // The walk is one floor call (a pool job when asynchronous); this only
  // shapes its listing. Node's callback and synchronous forms list a level at
  // a time; `fs.promises.readdir` keeps a stack and finishes the last
  // directory found first, which `depthFirst` reproduces.
  function readdirRecursive(base, options, depthFirst) {
    var withFileTypes = Boolean(options.withFileTypes);
    var raw = !meta.plainEncoding(options.encoding);
    var tree = host.readdir_tree(base, raw);
    var names = tree[0];
    var types = tree[1];
    var parents = tree[2];
    var isDirectory = tree[3];
    var dirs = [base];
    var prefixes = [""];
    var children = [[]];
    for (var j = 0; j < names.length; j++) {
      var name = raw ? meta.encodeName(names[j], options.encoding) : names[j];
      var dir = dirs[parents[j]];
      var prefix = prefixes[parents[j]];
      var relative = prefix === "" ? name : prefix + "/" + name;
      var entry = {
        value: withFileTypes ? getDirent(dir, name, types[j]) : relative,
        dir: -1,
      };
      if (isDirectory[j]) {
        entry.dir = dirs.length;
        dirs.push(join(dir, name));
        prefixes.push(relative);
        children.push([]);
      }
      children[parents[j]].push(entry);
    }
    var results = [];
    var pending = [0];
    var head = 0;
    while (depthFirst ? pending.length > 0 : head < pending.length) {
      var current = depthFirst ? pending.pop() : pending[head++];
      children[current].forEach(function (child) {
        results.push(child.value);
        if (child.dir !== -1) pending.push(child.dir);
      });
    }
    return results;
  }

  function readdirWith(path, options, depthFirst) {
    options = getOptions(options, {});
    path = getValidatedPath(path);
    if (options.recursive != null) {
      validateBoolean(options.recursive, "options.recursive");
    }
    if (options.recursive) return readdirRecursive(path, options, depthFirst);
    if (!options.withFileTypes) {
      var raw = !meta.plainEncoding(options.encoding);
      var names = host.readdir(path, raw);
      return raw
        ? names.map(function (n) { return meta.encodeName(n, options.encoding); })
        : names;
    }
    var found = listing(path, options);
    return found[0].map(function (name, i) {
      return getDirent(path, name, found[1][i]);
    });
  }

  function readdirSync(path, options) {
    return readdirWith(path, options, false);
  }

  // ---- mkdir / mkdtemp -------------------------------------------------------

  function mkdirSync(path, options) {
    var mode = 0o777;
    var recursive = false;
    if (typeof options === "number" || typeof options === "string") {
      mode = S.parseFileMode(options, "mode");
    } else if (options) {
      if (options.recursive !== undefined) {
        recursive = options.recursive;
        validateBoolean(recursive, "options.recursive");
      }
      if (options.mode !== undefined) {
        mode = S.parseFileMode(options.mode, "options.mode");
      }
    }
    path = getValidatedPath(path);
    if (recursive) return host.mkdirp(path, mode);
    host.mkdir(path, mode);
  }

  function mkdtempSync(prefix, options) {
    options = getOptions(options, {});
    prefix = getValidatedPath(prefix, "prefix");
    var raw = !meta.plainEncoding(options.encoding);
    return meta.encodeName(host.mkdtemp(prefix, raw), options.encoding);
  }

  // ---- opendir ---------------------------------------------------------------

  function closedError() {
    return nodeError(Error, "ERR_DIR_CLOSED", "Directory handle was closed");
  }

  // The listing is read when the directory is opened; entries come back one
  // by one and sorted bytewise (the OS order is unspecified in Node).
  var kState = Symbol("state");

  function Dir(path, entries) {
    Object.defineProperty(this, kState, {
      value: { path: path, entries: entries, index: 0, closed: false },
    });
  }
  Object.defineProperty(Dir.prototype, "path", {
    get: function () { return this[kState].path; },
    configurable: true,
  });
  Dir.prototype.readSync = function readSync() {
    var st = this[kState];
    if (st.closed) throw closedError();
    if (st.index >= st.entries[0].length) return null;
    var i = st.index++;
    return getDirent(st.path, st.entries[0][i], st.entries[1][i]);
  };
  Dir.prototype.closeSync = function closeSync() {
    var st = this[kState];
    if (st.closed) throw closedError();
    st.closed = true;
  };
  Dir.prototype[Symbol.dispose] = function () {
    if (!this[kState].closed) this.closeSync();
  };

  function opendirSync(path, options) {
    path = getValidatedPath(path);
    options = getOptions(options, { encoding: "utf8" });
    var bufferSize = options.bufferSize === undefined ? 32 : options.bufferSize;
    var raw = !meta.plainEncoding(options.encoding);
    var entries = host.readdir_types(path, raw, true);
    validateInteger(bufferSize, "options.bufferSize", 1, 4294967295);
    if (raw) {
      entries[0] = entries[0].map(function (n) {
        return meta.encodeName(n, options.encoding);
      });
    }
    return new Dir(path, entries);
  }

  // ---- rm ---------------------------------------------------------------------

  var RM_MESSAGES = {
    EACCES: "Permission denied",
    EPERM: "Operation not permitted",
    ENOTEMPTY: "Directory not empty",
    ENOTDIR: "Not a directory",
  };

  function rmSync(path, options) {
    var defaults = { force: false, recursive: false, retryDelay: 100, maxRetries: 0 };
    if (options !== undefined) {
      S.validateObject(options, "options");
      options = Object.assign({}, defaults, options);
    } else {
      options = defaults;
    }
    validateBoolean(options.force, "options.force");
    validateBoolean(options.recursive, "options.recursive");
    validateInteger(options.retryDelay, "options.retryDelay", 0, 2147483647);
    validateInteger(options.maxRetries, "options.maxRetries", 0, 4294967295);
    var st;
    if (!options.force || !options.recursive) {
      st = meta.lstatSync(path, { throwIfNoEntry: !options.force });
      if (st && st.isDirectory() && !options.recursive) {
        throw S.systemError("ERR_FS_EISDIR", "Path is a directory", {
          code: "EISDIR",
          message: "is a directory",
          path: path,
          syscall: "rm",
          errno: 21,
        });
      }
    }
    path = getValidatedPath(path);
    try {
      if (options.recursive) host.rm_all(path);
      else host.unlink(path);
    } catch (e) {
      if (e.code === "ENOENT" || (options.force && e.code === "ENOTDIR")) return;
      var text = RM_MESSAGES[e.code];
      if (text === undefined) throw e;
      var error = new Error(e.code + ", " + text + ": " + path + " '" + path + "'");
      error.errno = -e.errno;
      error.code = e.code;
      error.path = path;
      error.syscall = "rm";
      throw error;
    }
  }

  S.path = { normalize: normalize, join: join, isAbsolute: isAbsolute, dirname: dirname, resolve: resolve };
  S.readdirDepthFirst = function (path, options) {
    return readdirWith(path, options, true);
  };
  S.dirs = {
    Dir: Dir,
    Dirent: stats.Dirent,
    mkdirSync: mkdirSync,
    mkdtempSync: mkdtempSync,
    opendirSync: opendirSync,
    readdirSync: readdirSync,
    rmSync: rmSync,
  };
})();
//# sourceURL=node:fs
