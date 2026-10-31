// Node host-compat `fs` metadata half (issues #1217, #1218): stat/lstat/fstat,
// rmdir/unlink/rename, access and realpath on the `__host_fs_*` floor. Runs
// after node-fs-stats.js and parks the functions on
// `__node_fs_support.meta`; readdir and mkdir live in node-fs-dirs.js.

(function () {
  "use strict";

  var S = globalThis.__node_fs_support;
  if (!S) return;
  var host = S.host;
  var Buffer = S.Buffer;
  var hostConstants = S.hostConstants;
  var S_IFMT = S.S_IFMT;
  var nodeError = S.nodeError;
  var getStats = S.stats.getStats;
  var getValidatedPath = S.getValidatedPath;
  var getValidatedFd = S.getValidatedFd;
  var getOptions = S.getOptions;


  function statWith(kind, target, options) {
    var big = Boolean(options && options.bigint);
    try {
      return getStats(host.stat(target, kind, big));
    } catch (e) {
      if (
        options && options.throwIfNoEntry === false && kind !== 2 &&
        (e.code === "ENOENT" || (kind === 0 && e.code === "ENOTDIR"))
      ) {
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
    Stats: S.stats.Stats,
    encodeName: encodeName,
    plainEncoding: plainEncoding,
    statSync: statSync,
    lstatSync: lstatSync,
    fstatSync: fstatSync,
    rmdirSync: rmdirSync,
    unlinkSync: unlinkSync,
    renameSync: renameSync,
    accessSync: accessSync,
    realpathSync: realpathSync,
  };
})();
//# sourceURL=node:fs
