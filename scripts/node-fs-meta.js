// Node host-compat `fs` metadata half (issue #1217): Stats, stat/lstat/fstat,
// readdir, mkdir/rmdir/unlink/rename, access and realpath on the
// `__host_fs_*` floor. Runs between node-fs-support.js and node-fs-shim.js and
// parks the functions on `__node_fs_support.meta`. Stats/Dirent classes,
// mkdir `recursive`, `withFileTypes` and the rest of the sync surface are #1218.

(function () {
  "use strict";

  var S = globalThis.__node_fs_support;
  if (!S) return;
  var host = S.host;
  var Buffer = S.Buffer;
  var hostConstants = S.hostConstants;
  var S_IFMT = S.S_IFMT;
  var nodeError = S.nodeError;
  var notImplemented = S.notImplemented;
  var getValidatedPath = S.getValidatedPath;
  var getValidatedFd = S.getValidatedFd;
  var getOptions = S.getOptions;
  var parseFileMode = S.parseFileMode;


  var kAtime = Symbol("atime");
  var kMtime = Symbol("mtime");
  var kCtime = Symbol("ctime");
  var kBirthtime = Symbol("birthtime");

  function dateFromMs(ms) {
    return new Date(Math.round(ms));
  }

  function Stats(v) {
    this.dev = v[0];
    this.mode = v[1];
    this.nlink = v[2];
    this.uid = v[3];
    this.gid = v[4];
    this.rdev = v[5];
    this.blksize = v[6];
    this.ino = v[7];
    this.size = v[8];
    this.blocks = v[9];
    this.atimeMs = v[10] * 1e3 + v[11] / 1e6;
    this.mtimeMs = v[12] * 1e3 + v[13] / 1e6;
    this.ctimeMs = v[14] * 1e3 + v[15] / 1e6;
    this.birthtimeMs = v[16] * 1e3 + v[17] / 1e6;
  }

  function lazyDate(name, key, msName) {
    Object.defineProperty(Stats.prototype, name, {
      enumerable: true,
      configurable: true,
      get: function () {
        if (!Object.prototype.hasOwnProperty.call(this, key)) {
          Object.defineProperty(this, key, {
            value: dateFromMs(this[msName]),
            writable: true,
          });
        }
        return this[key];
      },
      set: function (value) {
        Object.defineProperty(this, name, {
          value: value,
          writable: true,
          enumerable: true,
          configurable: true,
        });
      },
    });
  }
  lazyDate("atime", kAtime, "atimeMs");
  lazyDate("mtime", kMtime, "mtimeMs");
  lazyDate("ctime", kCtime, "ctimeMs");
  lazyDate("birthtime", kBirthtime, "birthtimeMs");

  [
    ["isDirectory", "S_IFDIR"],
    ["isFile", "S_IFREG"],
    ["isBlockDevice", "S_IFBLK"],
    ["isCharacterDevice", "S_IFCHR"],
    ["isSymbolicLink", "S_IFLNK"],
    ["isFIFO", "S_IFIFO"],
    ["isSocket", "S_IFSOCK"],
  ].forEach(function (entry) {
    var type = hostConstants[entry[1]];
    Object.defineProperty(Stats.prototype, entry[0], {
      value: function () {
        return (this.mode & S_IFMT) === type;
      },
      writable: true,
      configurable: true,
    });
  });

  function statWith(kind, target, options) {
    if (options && options.bigint) throw notImplemented("bigint stat");
    try {
      return new Stats(host.stat(target, kind));
    } catch (e) {
      if (options && options.throwIfNoEntry === false && e.code === "ENOENT") {
        return undefined;
      }
      throw e;
    }
  }

  function statSync(path, options) {
    return statWith(0, getValidatedPath(path), options);
  }

  function lstatSync(path, options) {
    return statWith(1, getValidatedPath(path), options);
  }

  function fstatSync(fd, options) {
    return statWith(2, getValidatedFd(fd), options);
  }

  function plainEncoding(encoding) {
    return !encoding || encoding === "utf8" || encoding === "utf-8";
  }

  // `value` came from the floor as a string, or as a Uint8Array when the
  // caller's encoding is not utf8; Node returns Buffers or re-encoded strings.
  function encodeName(value, encoding) {
    if (typeof value === "string") {
      return encoding === "buffer" ? Buffer.from(value) : value;
    }
    var buffer = Buffer.from(value.buffer, value.byteOffset, value.length);
    return encoding === "buffer" ? buffer : buffer.toString(encoding);
  }

  function readdirSync(path, options) {
    options = getOptions(options, {});
    if (options.withFileTypes || options.recursive) {
      throw notImplemented("readdirSync withFileTypes/recursive");
    }
    var raw = !plainEncoding(options.encoding);
    var names = host.readdir(getValidatedPath(path), raw);
    return raw
      ? names.map(function (n) { return encodeName(n, options.encoding); })
      : names;
  }

  function mkdirSync(path, options) {
    var mode = 0o777;
    if (options !== undefined && options !== null && typeof options === "object") {
      if (options.recursive) throw notImplemented("mkdirSync recursive");
      mode = options.mode;
    } else {
      mode = options;
    }
    path = getValidatedPath(path);
    host.mkdir(path, parseFileMode(mode, "mode", 0o777));
  }

  function rmdirSync(path) {
    host.rmdir(getValidatedPath(path));
  }

  function unlinkSync(path) {
    host.unlink(getValidatedPath(path));
  }

  function renameSync(oldPath, newPath) {
    oldPath = getValidatedPath(oldPath, "oldPath");
    newPath = getValidatedPath(newPath, "newPath");
    host.rename(oldPath, newPath);
  }

  function accessSync(path, mode) {
    path = getValidatedPath(path);
    if (mode === undefined || mode === null) {
      mode = hostConstants.F_OK;
    } else if (typeof mode !== "number") {
      throw nodeError(
        TypeError,
        "ERR_INVALID_ARG_TYPE",
        "mode must be int32 or null/undefined"
      );
    } else if (!isFinite(mode)) {
      throw nodeError(RangeError, "ERR_OUT_OF_RANGE", "mode is out of range");
    } else if (mode < 0 || mode > 7) {
      throw nodeError(
        RangeError,
        "ERR_OUT_OF_RANGE",
        "mode is out of range: >= 0 && <= 7"
      );
    }
    host.access(path, mode);
  }

  function resolvePosix(from, to) {
    var path = to.charAt(0) === "/" ? to : from.replace(/\/+$/, "") + "/" + to;
    var out = [];
    path.split("/").forEach(function (seg) {
      if (seg === "" || seg === ".") return;
      if (seg === "..") out.pop();
      else out.push(seg);
    });
    return "/" + out.join("/");
  }

  // Node's lib/fs.js realpathSync, minus the optional cache: walk the path one
  // component at a time with lstat, expanding symlinks and starting over.
  function realpathSync(path, options) {
    options = getOptions(options, {});
    var p = typeof path === "string" ? path : getValidatedPath(path) + "";
    getValidatedPath(p);
    p = resolvePosix(process.cwd(), p);
    var knownHard = Object.create(null);
    var seenLinks = Object.create(null);
    var current = "/";
    var pos = 1;
    while (pos < p.length) {
      var next = p.indexOf("/", pos);
      var previous = current;
      var base;
      if (next === -1) {
        var last = p.slice(pos);
        current += last;
        base = previous + last;
        pos = p.length;
      } else {
        current += p.slice(pos, next + 1);
        base = previous + p.slice(pos, next);
        pos = next + 1;
      }
      if (knownHard[base]) continue;
      var stats = host.stat(base, 1);
      if ((stats[1] & S_IFMT) !== hostConstants.S_IFLNK) {
        knownHard[base] = true;
        continue;
      }
      var id = stats[0].toString(32) + ":" + stats[7].toString(32);
      var linkTarget = seenLinks[id];
      if (linkTarget === undefined) {
        host.stat(base, 0);
        linkTarget = host.readlink(base);
        seenLinks[id] = linkTarget;
      }
      p = resolvePosix(previous, linkTarget) + (pos < p.length ? "/" + p.slice(pos) : "");
      p = resolvePosix("/", p);
      current = "/";
      pos = 1;
    }
    return encodeName(p, options.encoding);
  }

  realpathSync.native = function native(path, options) {
    options = getOptions(options, {});
    var raw = !plainEncoding(options.encoding);
    return encodeName(host.realpath(getValidatedPath(path), raw), options.encoding);
  };

  S.meta = {
    Stats: Stats,
    statSync: statSync,
    lstatSync: lstatSync,
    fstatSync: fstatSync,
    readdirSync: readdirSync,
    mkdirSync: mkdirSync,
    rmdirSync: rmdirSync,
    unlinkSync: unlinkSync,
    renameSync: renameSync,
    accessSync: accessSync,
    realpathSync: realpathSync,
  };
})();
