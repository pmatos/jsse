// Node host-compat `fs.cpSync` (issue #1218): lib/internal/fs/cp/cp-sync.js
// with the C++ halves (`cpSyncCheckPaths`, `cpSyncOverrideFile`,
// `cpSyncCopyDir` of src/node_file.cc) written out in JavaScript on the
// `__host_fs_*` floor. Runs after node-fs-ops.js and parks `cpSync` on
// `__node_fs_support.cp`. Directory entries are visited in sorted order.

(function () {
  "use strict";

  var S = globalThis.__node_fs_support;
  if (!S) return;
  var host = S.host;
  var meta = S.meta;
  var ops = S.ops;
  var dirs = S.dirs;
  var P = S.path;
  var C = S.hostConstants;
  var nodeError = S.nodeError;
  var DIR = C.S_IFDIR;

  function typeOf(st) {
    return st[1] & S.S_IFMT;
  }

  // std::filesystem::status / symlink_status: undefined when the path is not there.
  function status(path, follow) {
    try {
      return host.stat(path, follow ? 0 : 1, false);
    } catch (e) {
      return undefined;
    }
  }

  function parentPath(path) {
    var cut = path.lastIndexOf("/");
    if (cut === -1) return "";
    if (cut === 0) return path.length > 1 ? "/" : "";
    return path.slice(0, cut);
  }

  function equivalent(a, b) {
    var x = status(a, true);
    var y = status(b, true);
    return Boolean(x && y && x[0] === y[0] && x[7] === y[7]);
  }

  // Rebuild a floor error as the one Node's C++ would throw for `syscall`.
  function reshape(e, syscall, path, dest) {
    var cut = e.message.indexOf(", " + e.syscall);
    var message = (cut === -1 ? e.code : e.message.slice(0, cut)) + ", " + syscall;
    if (path !== undefined) message += " '" + path + "'";
    if (dest !== undefined) message += " -> '" + dest + "'";
    e.message = message;
    e.syscall = syscall;
    if (path !== undefined) e.path = path;
    else delete e.path;
    if (dest !== undefined) e.dest = dest;
    else delete e.dest;
    return e;
  }

  var STRERROR = {
    EEXIST: "File exists",
    ENOENT: "No such file or directory",
    ENOTDIR: "Not a directory",
    EISDIR: "Is a directory",
    EACCES: "Permission denied",
    EPERM: "Operation not permitted",
    ENOTEMPTY: "Directory not empty",
    EINVAL: "Invalid argument",
    EXDEV: "Invalid cross-device link",
    ELOOP: "Too many levels of symbolic links",
    EROFS: "Read-only file system",
    ENOSPC: "No space left on device",
    ENAMETOOLONG: "File name too long",
  };

  // The ErrnoException the C++ cp halves throw: "CODE, strerror 'path'", positive errno.
  function errnoShape(e, path) {
    if (STRERROR[e.code] === undefined) return reshape(e, "cp", path);
    var error = new Error(e.code + ", " + STRERROR[e.code] + " '" + path + "'");
    error.errno = -e.errno;
    error.code = e.code;
    error.path = path;
    error.syscall = "cp";
    return error;
  }

  function cpError(code, message) {
    return nodeError(Error, code, message);
  }

  function checkPaths(src, dest, dereference, recursive) {
    var srcStatus;
    try {
      srcStatus = host.stat(src, dereference ? 1 : 0, false);
    } catch (e) {
      throw reshape(e, dereference ? "stat" : "lstat", src);
    }
    var destStatus = status(dest, dereference);
    var destExists = destStatus !== undefined;
    var srcType = typeOf(srcStatus);
    var srcIsDir = srcType === DIR || (dereference && srcType === C.S_IFLNK);
    if (destExists) {
      if (equivalent(src, dest)) {
        throw cpError("ERR_FS_CP_EINVAL", "src and dest cannot be the same " + dest);
      }
      var destIsDir = typeOf(destStatus) === DIR;
      if (srcIsDir && !destIsDir) {
        throw cpError(
          "ERR_FS_CP_DIR_TO_NON_DIR",
          "Cannot overwrite non-directory " + dest + " with directory " + src
        );
      }
      if (!srcIsDir && destIsDir) {
        throw cpError(
          "ERR_FS_CP_NON_DIR_TO_DIR",
          "Cannot overwrite directory " + dest + " with non-directory " + src
        );
      }
    }
    var srcSlash = src.charAt(src.length - 1) === "/" ? src : src + "/";
    var subdirOfSelf = function () {
      return cpError(
        "ERR_FS_CP_EINVAL",
        "Cannot copy " + srcSlash + " to a subdirectory of self " + dest
      );
    };
    if (srcIsDir && dest.slice(0, srcSlash.length) === srcSlash) throw subdirOfSelf();
    var destParent = parentPath(dest);
    if (
      parentPath(src) !== destParent && parentPath(destParent) !== "" &&
      parentPath(destParent) !== destParent
    ) {
      var a = status(src, true);
      var b = status(destParent, true);
      if (a && b && a[0] === b[0] && a[7] === b[7]) throw subdirOfSelf();
    }
    if (srcIsDir && !recursive) {
      throw cpError(
        "ERR_FS_EISDIR",
        "Recursive option not enabled, cannot copy a directory: " + srcSlash
      );
    }
    var special = {
      [C.S_IFSOCK]: ["ERR_FS_CP_SOCKET", "Cannot copy a socket file: "],
      [C.S_IFIFO]: ["ERR_FS_CP_FIFO_PIPE", "Cannot copy a FIFO pipe: "],
    }[srcType];
    if (special) throw cpError(special[0], special[1] + dest);
    var parentDir = parentPath(dest);
    if (parentDir !== "" && (!destExists || !status(parentDir, true))) {
      try {
        host.mkdirp(parentDir, 0o777);
      } catch (e) {
        // create_directories reports through an ignored error_code
      }
    }
  }

  function copyUtimes(src, dest) {
    var st = host.stat(src, 0, false);
    host.utimes(dest, st[10] + st[11] / 1e9, st[12] + st[13] / 1e9, 0);
  }

  function weaklyCanonical(path) {
    var rest = [];
    var prefix = path;
    for (;;) {
      try {
        var real = host.realpath(prefix, false);
        return P.normalize(rest.length ? real + "/" + rest.reverse().join("/") : real);
      } catch (e) {
        var slash = prefix.lastIndexOf("/");
        if (slash <= 0) return P.normalize(path);
        rest.push(prefix.slice(slash + 1));
        prefix = prefix.slice(0, slash);
      }
    }
  }

  function copyDirContents(src, dest, o) {
    var destDir = dest;
    var entries = host.readdir_types(src, false, false);
    for (var i = 0; i < entries[0].length; i++) {
      var name = entries[0][i];
      var type = entries[1][i];
      var from = src + "/" + name;
      var to = dest + "/" + name;
      if (type === 3) {
        copySymlink(from, to, destDir, src, o);
      } else if (type === 2) {
        try {
          host.mkdir(to, 0o777);
        } catch (e) {
          if (e.code !== "EEXIST") throw e;
        }
        copyDirContents(from, to, o);
      } else if (type === 1) {
        copyRegular(from, to, destDir, o);
      }
    }
  }

  function copySymlink(from, to, destDir, srcDir, o) {
    try {
      if (o.verbatimSymlinks) {
        host.symlink(host.readlink(from, false), to);
        return;
      }
      var target = host.readlink(from, false);
      var exists = status(to, true) !== undefined;
      if (exists) {
        var toStat = host.stat(to, 1, false);
        if (typeOf(toStat) === C.S_IFLNK) {
          var current = host.readlink(to, false);
          if (!o.dereference && isDirectoryOf(target) && P.isAbsolute(current) &&
              isInside(target, current)) {
            throw cpError("ERR_FS_CP_EINVAL", "Cannot copy " + target + " to a subdirectory of self " + current);
          }
          if (isDirectoryOf(to) && isInside(current, target)) {
            throw cpError("ERR_FS_CP_SYMLINK_TO_SUBDIRECTORY", "cannot overwrite " + current + " with " + target);
          }
          host.unlink(to);
        } else if (typeOf(toStat) === C.S_IFREG) {
          if (!o.dereference || (!o.force && o.errorOnExist)) {
            throw errnoShape({ code: "EEXIST", errno: -17 }, to);
          }
        }
      }
      var absolute = P.isAbsolute(target) ? target : P.resolve(srcDir, target);
      host.symlink(weaklyCanonical(absolute), to);
    } catch (e) {
      if (e.syscall && e.syscall !== "cp") throw errnoShape(e, destDir);
      throw e;
    }
  }

  function isDirectoryOf(path) {
    var st = status(path, true);
    return Boolean(st && typeOf(st) === DIR);
  }

  function isInside(src, dest) {
    var a = P.resolve("", src).split("/").filter(Boolean);
    var b = P.resolve("", dest).split("/").filter(Boolean);
    return a.length <= b.length && a.every(function (seg, i) { return b[i] === seg; });
  }

  function copyRegular(from, to, destDir, o) {
    var exists = status(to, false) !== undefined;
    if (exists) {
      if (o.force) {
        host.unlink(to);
      } else if (o.errorOnExist) {
        throw cpError(
          "ERR_FS_CP_EEXIST",
          "[ERR_FS_CP_EEXIST]: Target already exists: cp returned EEXIST (" + to + " already exists)"
        );
      } else {
        return;
      }
    }
    try {
      host.copyfile(from, to, 0);
    } catch (e) {
      throw errnoShape(e, destDir);
    }
    if (o.preserveTimestamps) copyUtimes(from, to);
  }

  function cpSyncCopyDir(src, dest, o) {
    try {
      host.mkdirp(dest, 0o777);
    } catch (e) {
      throw errnoShape(e, dest);
    }
    copyDirContents(src, dest, o);
  }

  function cpSyncOverrideFile(src, dest, mode, preserveTimestamps) {
    host.unlink(dest);
    try {
      host.copyfile(src, dest, mode);
    } catch (e) {
      throw mode === 0 ? errnoShape(e, dest) : reshape(e, "cp", src, dest);
    }
    if (preserveTimestamps) copyUtimes(src, dest);
  }

  function getStats(src, dest, o) {
    var statFn = o.dereference ? meta.statSync : meta.lstatSync;
    var srcStat = statFn(src);
    var destStat = statFn(dest, { bigint: true, throwIfNoEntry: false });
    if (srcStat.isDirectory() && o.recursive) {
      return onDir(srcStat, destStat, src, dest, o);
    } else if (srcStat.isFile() || srcStat.isCharacterDevice() || srcStat.isBlockDevice()) {
      return onFile(srcStat, destStat, src, dest, o);
    } else if (srcStat.isSymbolicLink()) {
      return onLink(destStat, src, dest, o.verbatimSymlinks);
    }
    throw new Error("unreachable fs.cpSync file type");
  }

  function onFile(srcStat, destStat, src, dest, o) {
    if (!destStat) {
      ops.copyFileSync(src, dest, o.mode);
      if (o.preserveTimestamps) {
        if ((srcStat.mode & 0o200) === 0) ops.chmodSync(dest, srcStat.mode | 0o200);
        var updated = meta.statSync(src);
        ops.utimesSync(dest, updated.atime, updated.mtime);
      }
      return ops.chmodSync(dest, srcStat.mode);
    }
    if (o.force) return cpSyncOverrideFile(src, dest, o.mode, o.preserveTimestamps);
    if (o.errorOnExist) {
      throw S.systemError("ERR_FS_CP_EEXIST", "Target already exists", {
        message: dest + " already exists",
        path: dest,
        syscall: "cp",
        errno: 17,
        code: "EEXIST",
      });
    }
  }

  function onDir(srcStat, destStat, src, dest, o) {
    if (!destStat) return copyDir(src, dest, o, true, srcStat.mode);
    return copyDir(src, dest, o);
  }

  function copyDir(src, dest, o, mkDir, srcMode) {
    if (!o.filter) return cpSyncCopyDir(src, dest, o);
    if (mkDir) dirs.mkdirSync(dest);
    var dir = dirs.opendirSync(src);
    try {
      var dirent;
      while ((dirent = dir.readSync()) !== null) {
        var srcItem = P.join(src, dirent.name);
        var destItem = P.join(dest, dirent.name);
        var shouldCopy = o.filter(srcItem, destItem);
        if (shouldCopy instanceof Promise) throw filterReturnError(shouldCopy);
        if (shouldCopy) getStats(srcItem, destItem, o);
      }
    } finally {
      dir.closeSync();
    }
    if (srcMode !== undefined) ops.chmodSync(dest, srcMode);
  }

  function onLink(destStat, src, dest, verbatimSymlinks) {
    var resolvedSrc = ops.readlinkSync(src);
    if (!verbatimSymlinks && !P.isAbsolute(resolvedSrc)) {
      resolvedSrc = P.resolve(P.dirname(src), resolvedSrc);
    }
    var type = isDirectoryOf(src) ? "dir" : "file";
    if (!destStat) return ops.symlinkSync(resolvedSrc, dest, type);
    var resolvedDest;
    try {
      resolvedDest = ops.readlinkSync(dest);
    } catch (err) {
      if (err.code === "EINVAL" || err.code === "UNKNOWN") {
        return ops.symlinkSync(resolvedSrc, dest, type);
      }
      throw err;
    }
    if (!P.isAbsolute(resolvedDest)) resolvedDest = P.resolve(P.dirname(dest), resolvedDest);
    if (type === "dir" && isInside(resolvedSrc, resolvedDest)) {
      throw cpErrorInfo("ERR_FS_CP_EINVAL", "cannot copy " + resolvedSrc + " to a subdirectory of self " + resolvedDest, dest);
    }
    if (meta.statSync(dest).isDirectory() && isInside(resolvedDest, resolvedSrc)) {
      throw cpErrorInfo("ERR_FS_CP_SYMLINK_TO_SUBDIRECTORY", "cannot overwrite " + resolvedDest + " with " + resolvedSrc, dest);
    }
    meta.unlinkSync(dest);
    return ops.symlinkSync(resolvedSrc, dest, type);
  }

  function cpErrorInfo(code, message, path) {
    return S.systemError(code, code === "ERR_FS_CP_EINVAL" ? "Invalid src or dest" : "Cannot overwrite symlink in subdirectory of self", {
      message: message, path: path, syscall: "cp", errno: 22, code: "EINVAL",
    });
  }

  function filterReturnError(value) {
    return nodeError(
      TypeError,
      "ERR_INVALID_RETURN_VALUE",
      'Expected boolean to be returned from the "filter" function but got ' +
        S.describeReceived(value).replace(/^ Received /, "") + "."
    );
  }

  function cpSync(src, dest, options) {
    var o = Object.assign({
      dereference: false,
      errorOnExist: false,
      filter: undefined,
      force: true,
      preserveTimestamps: false,
      recursive: false,
      verbatimSymlinks: false,
    }, validateOptions(options));
    src = S.getValidatedPath(src, "src");
    dest = S.getValidatedPath(dest, "dest");
    if (o.filter) {
      var shouldCopy = o.filter(src, dest);
      if (shouldCopy instanceof Promise) throw filterReturnError(shouldCopy);
      if (!shouldCopy) return;
    }
    checkPaths(src, dest, o.dereference, o.recursive);
    return getStats(src, dest, o);
  }

  function validateOptions(options) {
    if (options === undefined) return {};
    S.validateObject(options, "options");
    [
      "dereference", "errorOnExist", "force", "preserveTimestamps", "recursive",
      "verbatimSymlinks",
    ].forEach(function (key) {
      if (options[key] !== undefined) S.validateBoolean(options[key], "options." + key);
    });
    var o = Object.assign({}, options);
    var mode = o.mode || 0;
    S.validateInteger(mode, "mode", 0, 7);
    o.mode = mode;
    if (o.dereference === true && o.verbatimSymlinks === true) {
      throw nodeError(
        TypeError,
        "ERR_INCOMPATIBLE_OPTION_PAIR",
        'Option "dereference" cannot be used in combination with option "verbatimSymlinks"'
      );
    }
    if (o.filter !== undefined) S.validateFunction(o.filter, "options.filter");
    return o;
  }

  S.cpValidate = function (src, dest, options) {
    validateOptions(options);
    S.getValidatedPath(src, "src");
    S.getValidatedPath(dest, "dest");
  };
  S.cp = { cpSync: cpSync };
})();
