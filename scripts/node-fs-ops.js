// Node host-compat `fs` file operations (issue #1218): links, permissions,
// ownership, timestamps, truncate/fsync, copyFile and statfs on the
// `__host_fs_*` floor. Runs after node-fs-dirs.js and parks its exports on
// `__node_fs_support.ops`.

(function () {
  "use strict";

  var S = globalThis.__node_fs_support;
  if (!S) return;
  var host = S.host;
  var Buffer = S.Buffer;
  var nodeError = S.nodeError;
  var getOptions = S.getOptions;
  var getValidatedPath = S.getValidatedPath;
  var getValidatedFd = S.getValidatedFd;
  var parseFileMode = S.parseFileMode;
  var validateInteger = S.validateInteger;
  var MAX_USER_ID = 4294967295;

  function symlinkSync(target, path, type) {
    if (type !== undefined && type !== null && type !== "dir" && type !== "file" && type !== "junction") {
      throw S.invalidArgValue(
        "type",
        type,
        "must be one of: 'dir', 'file', 'junction', null, undefined"
      );
    }
    target = getValidatedPath(target, "target");
    path = getValidatedPath(path);
    host.symlink(target, path);
  }

  function readlinkSync(path, options) {
    options = getOptions(options, {});
    var raw = !S.meta.plainEncoding(options.encoding);
    var value = host.readlink(getValidatedPath(path), raw);
    return S.meta.encodeName(value, options.encoding);
  }

  function linkSync(existingPath, newPath) {
    existingPath = getValidatedPath(existingPath, "existingPath");
    newPath = getValidatedPath(newPath, "newPath");
    host.link(existingPath, newPath);
  }

  function chmodSync(path, mode) {
    path = getValidatedPath(path);
    host.chmod(path, parseFileMode(mode, "mode"), false);
  }

  function fchmodSync(fd, mode) {
    getValidatedFd(fd);
    host.chmod(fd, parseFileMode(mode, "mode"), true);
  }

  function chownWith(kind, target, uid, gid) {
    validateInteger(uid, "uid", -1, MAX_USER_ID);
    validateInteger(gid, "gid", -1, MAX_USER_ID);
    host.chown(target, uid, gid, kind);
  }

  function chownSync(path, uid, gid) {
    chownWith(0, getValidatedPath(path), uid, gid);
  }

  function lchownSync(path, uid, gid) {
    chownWith(1, getValidatedPath(path), uid, gid);
  }

  function fchownSync(fd, uid, gid) {
    chownWith(2, getValidatedFd(fd), uid, gid);
  }

  // Seconds as a (possibly fractional) number; negative means "now".
  function toUnixTimestamp(time) {
    if (typeof time === "string" && +time == time) return +time;
    if (Number.isFinite(time)) return time < 0 ? Date.now() / 1000 : time;
    if (time instanceof Date) return time.getTime() / 1000;
    throw S.nodeError(
      TypeError,
      "ERR_INVALID_ARG_TYPE",
      'The "time" argument must be an instance of Date or an Time in seconds.' +
        S.describeReceived(time)
    );
  }

  function utimesWith(kind, target, atime, mtime) {
    host.utimes(target, toUnixTimestamp(atime), toUnixTimestamp(mtime), kind);
  }

  function utimesSync(path, atime, mtime) {
    utimesWith(0, getValidatedPath(path), atime, mtime);
  }

  function lutimesSync(path, atime, mtime) {
    utimesWith(1, getValidatedPath(path), atime, mtime);
  }

  function futimesSync(fd, atime, mtime) {
    utimesWith(2, getValidatedFd(fd), atime, mtime);
  }

  function ftruncateSync(fd, len) {
    if (len === undefined) len = 0;
    getValidatedFd(fd);
    validateInteger(len, "len", -S.MAX_SAFE, S.MAX_SAFE);
    host.ftruncate(fd, len < 0 ? 0 : len);
  }

  function truncateSync(path, len) {
    if (len === undefined) len = 0;
    var fd = host.open(getValidatedPath(path), S.stringToFlags("r+"), 0o666);
    try {
      ftruncateSync(fd, len);
    } finally {
      host.close(fd);
    }
  }

  function fsyncSync(fd) {
    host.fsync(getValidatedFd(fd), false);
  }

  function fdatasyncSync(fd) {
    host.fsync(getValidatedFd(fd), true);
  }

  function copyMode(mode) {
    if (mode === undefined || mode === null) return 0;
    if (typeof mode !== "number") {
      throw nodeError(TypeError, "ERR_INVALID_ARG_TYPE", "mode must be int32 or null/undefined");
    }
    if (!isFinite(mode)) {
      throw nodeError(RangeError, "ERR_OUT_OF_RANGE", "mode is out of range");
    }
    if (mode < 0 || mode > 7) {
      throw nodeError(RangeError, "ERR_OUT_OF_RANGE", "mode is out of range: >= 0 && <= 7");
    }
    return mode;
  }

  function copyFileSync(src, dest, mode) {
    src = getValidatedPath(src, "src");
    dest = getValidatedPath(dest, "dest");
    host.copyfile(src, dest, copyMode(mode));
  }

  function statfsSync(path, options) {
    var values = host.statfs(getValidatedPath(path));
    if (options && options.bigint) values = values.map(BigInt);
    return new S.stats.StatFs(
      values[0], values[1], values[2], values[3], values[4], values[5], values[6], values[7]
    );
  }

  S.copyMode = copyMode;
  S.toUnixTimestamp = toUnixTimestamp;
  S.ops = {
    chmodSync: chmodSync,
    chownSync: chownSync,
    copyFileSync: copyFileSync,
    fchmodSync: fchmodSync,
    fchownSync: fchownSync,
    fdatasyncSync: fdatasyncSync,
    fsyncSync: fsyncSync,
    ftruncateSync: ftruncateSync,
    futimesSync: futimesSync,
    lchownSync: lchownSync,
    linkSync: linkSync,
    lutimesSync: lutimesSync,
    readlinkSync: readlinkSync,
    statfsSync: statfsSync,
    symlinkSync: symlinkSync,
    truncateSync: truncateSync,
    utimesSync: utimesSync,
  };
})();
