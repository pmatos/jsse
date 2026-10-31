// Node host-compat `fs` support (issue #1217): the floor snapshot, Node's
// error constructors and the argument validators shared by node-fs-meta.js and
// node-fs-shim.js. Loaded first of the three (see node_prelude.rs). A no-op
// unless the floor exists (jsse `--node`), so real Node keeps its own `fs`.
// It parks its exports on `globalThis.__node_fs_support`, which node-fs-shim.js
// deletes once the `fs` object is assembled.

(function () {
  "use strict";

  if (
    typeof __host_fs_open === "undefined" ||
    typeof globalThis.Buffer !== "function"
  ) {
    return;
  }

  var host = {};
  [
    "open", "close", "read", "write", "read_file", "write_file", "stat",
    "exists", "access", "readdir", "readlink", "realpath", "mkdir", "rmdir",
    "unlink", "rename", "constants", "symlink", "link", "chmod", "chown",
    "utimes", "ftruncate", "fsync", "mkdtemp", "statfs", "copyfile",
    "readdir_types", "mkdirp", "rm_all", "readdir_tree",
  ].forEach(function (name) {
    host[name] = globalThis["__host_fs_" + name];
    delete globalThis["__host_fs_" + name];
  });

  var Buffer = globalThis.Buffer;
  var inspect = globalThis.util.inspect;
  var hostConstants = host.constants();
  var O_RDONLY = hostConstants.O_RDONLY;
  var S_IFMT = hostConstants.S_IFMT;
  var MAX_SAFE = Number.MAX_SAFE_INTEGER;

  // ---- errors ----------------------------------------------------------

  function nodeError(Ctor, code, message) {
    var e = new Ctor(message);
    e.code = code;
    return e;
  }

  function describeReceived(v) {
    if (v === undefined || v === null) return " Received " + v;
    if (typeof v === "function") return " Received function " + v.name;
    if (typeof v === "object") {
      var ctor = v.constructor;
      if (ctor && typeof ctor.name === "string" && ctor.name) {
        return " Received an instance of " + ctor.name;
      }
      return " Received [object]";
    }
    var shown;
    if (typeof v === "string") {
      shown = v.length > 28 ? v.slice(0, 25) + "..." : v;
      shown = "'" + shown + "'";
    } else if (typeof v === "bigint") {
      shown = v + "n";
    } else {
      shown = String(v);
    }
    return " Received type " + typeof v + " (" + shown + ")";
  }

  function invalidArgType(name, expected, value) {
    var kind = name.indexOf(".") === -1 ? "argument" : "property";
    return nodeError(
      TypeError,
      "ERR_INVALID_ARG_TYPE",
      'The "' + name + '" ' + kind + " must be " + expected + "." +
        describeReceived(value)
    );
  }

  var STR_ESCAPES = { 8: "\\b", 9: "\\t", 10: "\\n", 12: "\\f", 13: "\\r" };

  function quoteString(s) {
    var quote = s.indexOf("'") !== -1 && s.indexOf('"') === -1 ? '"' : "'";
    var out = "";
    for (var i = 0; i < s.length; i++) {
      var c = s.charCodeAt(i);
      var ch = s.charAt(i);
      if (ch === quote || ch === "\\") out += "\\" + ch;
      else if (STR_ESCAPES[c]) out += STR_ESCAPES[c];
      else if (c < 32 || c === 127) {
        out += "\\x" + (c < 16 ? "0" : "") + c.toString(16).toUpperCase();
      } else out += ch;
    }
    return quote + out + quote;
  }

  function shown(v) {
    if (typeof v === "string") return quoteString(v);
    if (Buffer.isBuffer(v)) {
      var hex = [];
      for (var i = 0; i < v.length && i < 50; i++) {
        hex.push((v[i] < 16 ? "0" : "") + v[i].toString(16));
      }
      var more = v.length > 50 ? " ... " + (v.length - 50) + " more byte" + (v.length - 50 > 1 ? "s" : "") : "";
      return "<Buffer " + hex.join(" ") + more + ">";
    }
    return inspect(v);
  }

  function invalidArgValue(name, value, reason) {
    return nodeError(
      TypeError,
      "ERR_INVALID_ARG_VALUE",
      "The " + (name.indexOf(".") === -1 ? "argument" : "property") + " '" +
        name + "' " + (reason || "is invalid") +
        ". Received " + shown(value)
    );
  }

  function separated(n) {
    if (!Number.isInteger(n) || Math.abs(n) <= 4294967296) return String(n);
    var digits = String(Math.abs(n));
    var out = "";
    for (var i = digits.length; i > 3; i -= 3) {
      out = "_" + digits.slice(i - 3, i) + out;
    }
    return (n < 0 ? "-" : "") + digits.slice(0, i) + out;
  }

  function outOfRange(name, range, value) {
    return nodeError(
      RangeError,
      "ERR_OUT_OF_RANGE",
      'The value of "' + name + '" is out of range. It must be ' + range +
        ". Received " + (typeof value === "number" ? separated(value) : shown(value))
    );
  }

  function notImplemented(what) {
    return nodeError(
      Error,
      "ERR_METHOD_NOT_IMPLEMENTED",
      "The " + what + " method is not implemented"
    );
  }

  // Node's SystemError (lib/internal/errors.js): `info` holds the context and
  // the message reads `<prefix>: <syscall> returned <code> (<message>) <path>`.
  function systemError(code, prefix, ctx) {
    var message = prefix + ": " + ctx.syscall + " returned " + ctx.code +
      " (" + ctx.message + ")";
    if (ctx.path !== undefined) message += " " + ctx.path;
    if (ctx.dest !== undefined) message += " => " + ctx.dest;
    var e = new Error(message);
    Object.defineProperty(e, "name", {
      value: "SystemError",
      writable: true,
      configurable: true,
    });
    e.code = code;
    e.info = ctx;
    e.errno = ctx.errno;
    e.syscall = ctx.syscall;
    if (ctx.path !== undefined) e.path = ctx.path;
    if (ctx.dest !== undefined) e.dest = ctx.dest;
    return e;
  }

  // ---- validators ------------------------------------------------------

  function validateBoolean(value, name) {
    if (typeof value !== "boolean") {
      throw invalidArgType(name, "of type boolean", value);
    }
  }

  function validateFunction(value, name) {
    if (typeof value !== "function") {
      throw invalidArgType(name, "of type function", value);
    }
  }

  function validateObject(value, name) {
    if (value === null || typeof value !== "object" || Array.isArray(value)) {
      throw invalidArgType(name, "of type object", value);
    }
  }

  function validateInteger(value, name, min, max) {
    if (typeof value !== "number") {
      throw invalidArgType(name, "of type number", value);
    }
    if (!Number.isInteger(value)) {
      throw outOfRange(name, "an integer", value);
    }
    if (value < min || value > max) {
      throw outOfRange(name, ">= " + min + " && <= " + max, value);
    }
  }

  function getValidatedFd(fd, name) {
    validateInteger(fd, name || "fd", 0, 2147483647);
    return fd;
  }

  function isInt32(v) {
    return typeof v === "number" && (v | 0) === v;
  }

  function isView(v) {
    return ArrayBuffer.isView(v);
  }

  function bytesOf(view) {
    return view instanceof Uint8Array
      ? view
      : new Uint8Array(view.buffer, view.byteOffset, view.byteLength);
  }

  function isURL(v) {
    return Boolean(
      v && v.href && v.protocol && v.auth === undefined && v.path === undefined
    );
  }

  function fileURLToPath(url) {
    if (url.protocol !== "file:") {
      throw nodeError(
        TypeError,
        "ERR_INVALID_URL_SCHEME",
        "The URL must be of scheme file"
      );
    }
    if (url.hostname !== "") {
      throw nodeError(
        TypeError,
        "ERR_INVALID_FILE_URL_HOST",
        'File URL host must be "localhost" or empty on ' + process.platform
      );
    }
    var pathname = url.pathname;
    for (var n = 0; n < pathname.length; n++) {
      if (
        pathname.charCodeAt(n) === 37 &&
        pathname.charAt(n + 1) === "2" &&
        pathname.charAt(n + 2).toLowerCase() === "f"
      ) {
        throw nodeError(
          TypeError,
          "ERR_INVALID_FILE_URL_PATH",
          "File URL path must not include encoded / characters"
        );
      }
    }
    return decodeURIComponent(pathname);
  }

  var NULL_BYTES = "must be a string, Uint8Array, or URL without null bytes";

  function getValidatedPath(path, name) {
    name = name || "path";
    if (isURL(path)) path = fileURLToPath(path);
    if (typeof path === "string") {
      if (path.indexOf("\u0000") !== -1) {
        throw invalidArgValue(name, path, NULL_BYTES);
      }
      return path;
    }
    if (path instanceof Uint8Array) {
      if (path.includes(0)) throw invalidArgValue(name, path, NULL_BYTES);
      return path;
    }
    throw invalidArgType(
      name,
      "of type string or an instance of Buffer or URL",
      path
    );
  }

  function assertEncoding(encoding) {
    if (encoding && !Buffer.isEncoding(encoding)) {
      throw invalidArgValue("encoding", encoding, "is invalid encoding");
    }
  }

  function getOptions(options, defaults) {
    if (options === undefined || options === null || typeof options === "function") {
      return defaults;
    }
    if (typeof options === "string") {
      options = Object.assign({}, defaults, { encoding: options });
    } else if (typeof options !== "object") {
      throw invalidArgType("options", "one of type string or object", options);
    }
    if (options.encoding !== "buffer") assertEncoding(options.encoding);
    return options;
  }

  function stringToFlags(flags, name) {
    if (flags === undefined || flags === null) return O_RDONLY;
    if (typeof flags === "number") return flags;
    var C = hostConstants;
    switch (flags) {
      case "r": return C.O_RDONLY;
      case "rs":
      case "sr": return C.O_RDONLY | C.O_SYNC;
      case "r+": return C.O_RDWR;
      case "rs+":
      case "sr+": return C.O_RDWR | C.O_SYNC;
      case "w": return C.O_TRUNC | C.O_CREAT | C.O_WRONLY;
      case "wx":
      case "xw": return C.O_TRUNC | C.O_CREAT | C.O_WRONLY | C.O_EXCL;
      case "w+": return C.O_TRUNC | C.O_CREAT | C.O_RDWR;
      case "wx+":
      case "xw+": return C.O_TRUNC | C.O_CREAT | C.O_RDWR | C.O_EXCL;
      case "a": return C.O_APPEND | C.O_CREAT | C.O_WRONLY;
      case "ax":
      case "xa": return C.O_APPEND | C.O_CREAT | C.O_WRONLY | C.O_EXCL;
      case "as":
      case "sa": return C.O_APPEND | C.O_CREAT | C.O_WRONLY | C.O_SYNC;
      case "a+": return C.O_APPEND | C.O_CREAT | C.O_RDWR;
      case "ax+":
      case "xa+": return C.O_APPEND | C.O_CREAT | C.O_RDWR | C.O_EXCL;
      case "as+":
      case "sa+": return C.O_APPEND | C.O_CREAT | C.O_RDWR | C.O_SYNC;
    }
    throw invalidArgValue(name || "flags", flags);
  }

  function parseFileMode(value, name, def) {
    if (value === undefined || value === null) value = def;
    if (typeof value === "string") {
      if (!/^[0-7]+$/.test(value)) {
        throw invalidArgValue(
          name,
          value,
          "must be a 32-bit unsigned integer or an octal string"
        );
      }
      value = parseInt(value, 8);
    }
    validateInteger(value, name, 0, 4294967295);
    return value;
  }

  function validatePosition(position) {
    if (position === undefined || position === null) return -1;
    if (typeof position === "bigint") {
      if (position < -1n || position > 9223372036854775807n) {
        throw outOfRange("position", ">= -1 && <= 9223372036854775807", position);
      }
      return Number(position);
    }
    if (typeof position !== "number" || !Number.isInteger(position)) {
      throw invalidArgType("position", "of type bigint or integer", position);
    }
    if (position < -1 || position > MAX_SAFE) {
      throw outOfRange("position", ">= -1 && <= " + MAX_SAFE, position);
    }
    return position;
  }

  function validateBuffer(buffer) {
    if (!isView(buffer)) {
      throw invalidArgType(
        "buffer",
        "an instance of Buffer, TypedArray, or DataView",
        buffer
      );
    }
  }

  function validateDataArg(data, name) {
    if (typeof data !== "string" && !isView(data)) {
      throw invalidArgType(
        name,
        "of type string or an instance of Buffer, TypedArray, or DataView",
        data
      );
    }
  }

  Object.defineProperty(globalThis, "__node_fs_support", {
    value: {
      host: host,
      Buffer: Buffer,
      hostConstants: hostConstants,
      MAX_SAFE: MAX_SAFE,
      S_IFMT: S_IFMT,
      O_RDONLY: O_RDONLY,
      nodeError: nodeError,
      invalidArgType: invalidArgType,
      describeReceived: describeReceived,
      invalidArgValue: invalidArgValue,
      outOfRange: outOfRange,
      notImplemented: notImplemented,
      validateInteger: validateInteger,
      validateBoolean: validateBoolean,
      validateFunction: validateFunction,
      validateObject: validateObject,
      systemError: systemError,
      getValidatedFd: getValidatedFd,
      isInt32: isInt32,
      isView: isView,
      bytesOf: bytesOf,
      getValidatedPath: getValidatedPath,
      assertEncoding: assertEncoding,
      getOptions: getOptions,
      stringToFlags: stringToFlags,
      parseFileMode: parseFileMode,
      validatePosition: validatePosition,
      validateBuffer: validateBuffer,
      validateDataArg: validateDataArg,
    },
    writable: true,
    configurable: true,
  });
})();
//# sourceURL=node:internal/fs/utils
