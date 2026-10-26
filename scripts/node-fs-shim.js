// Node host-compat `fs` basics (issue #1217): the synchronous core on top of
// the `__host_fs_*` floor. Loaded last of the three fs scripts (see
// node_prelude.rs). The module is registered on the provisional builtin
// registry `globalThis.__node_builtins` until the CommonJS loader exists; see
// docs/adr/2026-10-26-0854-node-host-mode.md.

(function () {
  "use strict";

  var S = globalThis.__node_fs_support;
  if (!S) return;
  delete globalThis.__node_fs_support;
  var host = S.host;
  var Buffer = S.Buffer;
  var hostConstants = S.hostConstants;
  var MAX_SAFE = S.MAX_SAFE;
  var isInt32 = S.isInt32;
  var isView = S.isView;
  var bytesOf = S.bytesOf;
  var getValidatedPath = S.getValidatedPath;
  var getValidatedFd = S.getValidatedFd;
  var assertEncoding = S.assertEncoding;
  var getOptions = S.getOptions;
  var stringToFlags = S.stringToFlags;
  var parseFileMode = S.parseFileMode;
  var validateInteger = S.validateInteger;
  var validatePosition = S.validatePosition;
  var validateBuffer = S.validateBuffer;
  var validateDataArg = S.validateDataArg;
  var invalidArgType = S.invalidArgType;
  var nodeError = S.nodeError;
  var invalidArgValue = S.invalidArgValue;
  var outOfRange = S.outOfRange;
  var meta = S.meta;


  function isUtf8(encoding) {
    return encoding === "utf8" || encoding === "utf-8";
  }

  function toBuffer(u8) {
    return Buffer.from(u8.buffer, u8.byteOffset, u8.length);
  }

  function readFileSync(path, options) {
    options = getOptions(options, { flag: "r" });
    var target = isInt32(path)
      ? getValidatedFd(path)
      : getValidatedPath(path);
    var flags = stringToFlags(options.flag);
    var encoding = options.encoding;
    if (isUtf8(encoding)) return host.read_file(target, flags, true);
    var buffer = toBuffer(host.read_file(target, flags, false));
    if (!encoding) return buffer;
    if (!Buffer.isEncoding(encoding)) {
      throw nodeError(TypeError, "ERR_UNKNOWN_ENCODING", "Unknown encoding: " + encoding);
    }
    return buffer.toString(encoding);
  }

  function writeFileSync(path, data, options) {
    options = getOptions(options, { encoding: "utf8", mode: 0o666, flag: "w" });
    validateDataArg(data, "data");
    var target = isInt32(path)
      ? getValidatedFd(path)
      : getValidatedPath(path);
    var flags = stringToFlags(options.flag || "w");
    var mode = parseFileMode(options.mode, "mode", 0o666);
    if (typeof data === "string") {
      var enc = options.encoding || "utf8";
      if (!isUtf8(enc)) data = Buffer.from(data, enc);
    } else {
      data = bytesOf(data);
    }
    host.write_file(target, data, flags, mode);
  }

  function appendFileSync(path, data, options) {
    options = getOptions(options, { encoding: "utf8", mode: 0o666, flag: "a" });
    if (!options.flag || isInt32(path)) {
      options = Object.assign({}, options, { flag: "a" });
    }
    writeFileSync(path, data, options);
  }

  function existsSync(path) {
    try {
      path = getValidatedPath(path);
    } catch (e) {
      return false;
    }
    return host.exists(path);
  }

  function openSync(path, flags, mode) {
    path = getValidatedPath(path);
    return host.open(
      path,
      stringToFlags(flags),
      parseFileMode(mode, "mode", 0o666)
    );
  }

  function closeSync(fd) {
    host.close(getValidatedFd(fd));
  }

  function readSync(fd, buffer, offsetOrOptions, length, position) {
    getValidatedFd(fd);
    validateBuffer(buffer);
    var offset = offsetOrOptions;
    if (arguments.length <= 3 || typeof offsetOrOptions === "object") {
      if (offsetOrOptions !== undefined) {
        if (typeof offsetOrOptions !== "object") {
          throw invalidArgType("options", "of type object", offsetOrOptions);
        }
      }
      var o = offsetOrOptions || {};
      offset = o.offset === undefined ? 0 : o.offset;
      length = o.length === undefined ? buffer.byteLength - offset : o.length;
      position = o.position === undefined ? null : o.position;
    }
    if (offset === undefined) offset = 0;
    else validateInteger(offset, "offset", 0, MAX_SAFE);
    length |= 0;
    if (length === 0) return 0;
    if (buffer.byteLength === 0) {
      throw invalidArgValue("buffer", buffer, "is empty and cannot be written");
    }
    if (offset < 0) throw outOfRange("offset", ">= 0", offset);
    if (length < 0) throw outOfRange("length", ">= 0", length);
    if (offset + length > buffer.byteLength) {
      throw outOfRange("length", "<= " + (buffer.byteLength - offset), length);
    }
    return host.read(fd, bytesOf(buffer), offset, length, validatePosition(position));
  }

  function writeSync(fd, buffer, offsetOrOptions, length, position) {
    getValidatedFd(fd);
    if (isView(buffer)) {
      var offset = offsetOrOptions;
      if (position === undefined) position = null;
      if (typeof offset === "object" && offset !== null) {
        var o = offset;
        offset = o.offset === undefined ? 0 : o.offset;
        length = o.length === undefined ? buffer.byteLength - offset : o.length;
        position = o.position === undefined ? null : o.position;
      }
      if (offset === undefined || offset === null) offset = 0;
      else validateInteger(offset, "offset", 0, MAX_SAFE);
      if (typeof length !== "number") length = buffer.byteLength - offset;
      if (offset > buffer.byteLength) {
        throw outOfRange("offset", "<= " + buffer.byteLength, offset);
      }
      if (length > buffer.byteLength - offset) {
        throw outOfRange("length", "<= " + (buffer.byteLength - offset), length);
      }
      if (length < 0) throw outOfRange("length", ">= 0", length);
      validateInteger(length, "length", 0, 2147483647);
      return host.write(fd, bytesOf(buffer), offset, length, validatePosition(position));
    }
    validateDataArg(buffer, "buffer");
    var encoding = length;
    if (!encoding || !Buffer.isEncoding(encoding)) encoding = "utf8";
    else if (/^hex$/i.test(encoding) && buffer.length % 2 !== 0) {
      throw invalidArgValue(
        "encoding",
        encoding,
        "is invalid for data of length " + buffer.length
      );
    }
    var bytes = Buffer.from(buffer, encoding);
    return host.write(
      fd,
      bytes,
      0,
      bytes.length,
      validatePosition(offsetOrOptions)
    );
  }


  // ---- constants and registration -----------------------------------------

  var constants = {
    UV_FS_SYMLINK_DIR: 1,
    UV_FS_SYMLINK_JUNCTION: 2,
    O_RDONLY: hostConstants.O_RDONLY,
    O_WRONLY: hostConstants.O_WRONLY,
    O_RDWR: hostConstants.O_RDWR,
    UV_DIRENT_UNKNOWN: 0,
    UV_DIRENT_FILE: 1,
    UV_DIRENT_DIR: 2,
    UV_DIRENT_LINK: 3,
    UV_DIRENT_FIFO: 4,
    UV_DIRENT_SOCKET: 5,
    UV_DIRENT_CHAR: 6,
    UV_DIRENT_BLOCK: 7,
  };
  [
    "S_IFMT", "S_IFREG", "S_IFDIR", "S_IFCHR", "S_IFBLK", "S_IFIFO", "S_IFLNK",
    "S_IFSOCK", "O_CREAT", "O_EXCL",
  ].forEach(function (k) { constants[k] = hostConstants[k]; });
  constants.UV_FS_O_FILEMAP = 0;
  [
    "O_NOCTTY", "O_TRUNC", "O_APPEND", "O_DIRECTORY", "O_NOATIME",
    "O_NOFOLLOW", "O_SYNC", "O_DSYNC", "O_DIRECT", "O_NONBLOCK", "S_IRWXU",
    "S_IRUSR", "S_IWUSR", "S_IXUSR", "S_IRWXG", "S_IRGRP", "S_IWGRP",
    "S_IXGRP", "S_IRWXO", "S_IROTH", "S_IWOTH", "S_IXOTH", "F_OK", "R_OK",
    "W_OK", "X_OK",
  ].forEach(function (k) {
    if (k in hostConstants) constants[k] = hostConstants[k];
  });
  constants.UV_FS_COPYFILE_EXCL = 1;
  constants.COPYFILE_EXCL = 1;
  constants.UV_FS_COPYFILE_FICLONE = 2;
  constants.COPYFILE_FICLONE = 2;
  constants.UV_FS_COPYFILE_FICLONE_FORCE = 4;
  constants.COPYFILE_FICLONE_FORCE = 4;

  var fs = {
    Stats: meta.Stats,
    accessSync: meta.accessSync,
    appendFileSync: appendFileSync,
    closeSync: closeSync,
    constants: constants,
    existsSync: existsSync,
    fstatSync: meta.fstatSync,
    lstatSync: meta.lstatSync,
    mkdirSync: meta.mkdirSync,
    openSync: openSync,
    readFileSync: readFileSync,
    readSync: readSync,
    readdirSync: meta.readdirSync,
    realpathSync: meta.realpathSync,
    renameSync: meta.renameSync,
    rmdirSync: meta.rmdirSync,
    statSync: meta.statSync,
    unlinkSync: meta.unlinkSync,
    writeFileSync: writeFileSync,
    writeSync: writeSync,
  };

  if (!Object.prototype.hasOwnProperty.call(globalThis, "__node_builtins")) {
    Object.defineProperty(globalThis, "__node_builtins", {
      value: Object.create(null),
      writable: true,
      configurable: true,
    });
  }
  globalThis.__node_builtins.fs = fs;
})();
