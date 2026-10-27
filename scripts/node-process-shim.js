// Node host-compat `process` surface (issue #1215) on top of the --node floor:
// argv, argv0, execPath, env, cwd()/chdir(), exitCode and exit(). Loaded AFTER
// node-shim.js (see node_prelude.rs), which creates `globalThis.process`. A no-op unless the
// floor primitives exist (jsse `--node`), so real Node and plain jsse keep their
// own `process`. See docs/adr/2026-10-26-0854-node-host-mode.md.

(function () {
  "use strict";

  if (
    typeof __host_argv === "undefined" ||
    typeof globalThis.process !== "object" ||
    globalThis.process === null
  ) {
    return;
  }

  if (!("global" in globalThis)) globalThis.global = globalThis;

  var hostArgv = __host_argv;
  var hostArgv0 = __host_argv0;
  var hostEnvSnapshot = __host_env_snapshot;
  var hostCwd = __host_cwd;
  var hostChdir = __host_chdir;
  var hostSetExitCode = __host_set_exit_code;
  var hostExit = __host_exit;
  // Keep the primitives private to the shim, like the other metadata hooks.
  delete globalThis.__host_argv;
  delete globalThis.__host_argv0;
  delete globalThis.__host_env_snapshot;
  delete globalThis.__host_cwd;
  delete globalThis.__host_chdir;
  delete globalThis.__host_set_exit_code;

  var proc = globalThis.process;

  function describeReceived(v) {
    if (v === undefined || v === null) return " Received " + v;
    if (typeof v === "function") {
      return " Received function " + v.name;
    }
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

  function nodeError(Ctor, code, message) {
    var e = new Ctor(message);
    e.code = code;
    return e;
  }

  function validateExitCode(v) {
    if (v === undefined || v === null) return undefined;
    var n = v;
    if (typeof v === "string" && v.trim() !== "" && !isNaN(Number(v))) {
      n = Number(v);
    }
    if (typeof n !== "number") {
      throw nodeError(
        TypeError,
        "ERR_INVALID_ARG_TYPE",
        'The "code" argument must be of type number.' + describeReceived(v)
      );
    }
    if (!Number.isInteger(n)) {
      throw nodeError(
        RangeError,
        "ERR_OUT_OF_RANGE",
        'The value of "code" is out of range. It must be an integer. Received ' +
          n
      );
    }
    return n;
  }

  var exitCode;
  Object.defineProperty(proc, "exitCode", {
    get: function () {
      return exitCode;
    },
    set: function (v) {
      exitCode = validateExitCode(v);
      hostSetExitCode(exitCode);
    },
    enumerable: true,
    configurable: false,
  });

  proc.exit = function exit(code) {
    if (code !== undefined) proc.exitCode = code;
    hostExit(exitCode === undefined ? 0 : exitCode);
  };

  var argv = hostArgv();
  proc.argv = argv;
  proc.argv0 = hostArgv0();
  proc.execPath = argv[0];

  proc.cwd = function cwd() {
    return hostCwd();
  };
  proc.chdir = function chdir(directory) {
    if (typeof directory !== "string") {
      throw nodeError(
        TypeError,
        "ERR_INVALID_ARG_TYPE",
        'The "directory" argument must be of type string.' +
          describeReceived(directory)
      );
    }
    hostChdir(directory);
  };

  var envTarget = {};
  var snapshot = hostEnvSnapshot();
  for (var i = 0; i + 1 < snapshot.length; i += 2) {
    envTarget[snapshot[i]] = snapshot[i + 1];
  }

  function envString(v) {
    return "" + v;
  }

  proc.env = new Proxy(envTarget, {
    set: function (target, key, value) {
      if (typeof key === "symbol") envString(key);
      target[key] = envString(value);
      return true;
    },
    defineProperty: function (target, key, desc) {
      if ("get" in desc || "set" in desc) {
        throw nodeError(
          TypeError,
          "ERR_INVALID_OBJECT_DEFINE_PROPERTY",
          "'process.env' does not accept an accessor(getter/setter) descriptor"
        );
      }
      if (!desc.configurable || !desc.writable || !desc.enumerable) {
        throw nodeError(
          TypeError,
          "ERR_INVALID_OBJECT_DEFINE_PROPERTY",
          "'process.env' only accepts a configurable, writable, and " +
            "enumerable data descriptor"
        );
      }
      target[key] = envString(desc.value);
      return true;
    },
    deleteProperty: function (target, key) {
      delete target[key];
      return true;
    },
  });
})();
