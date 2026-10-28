// Builtin-module registry for `--node` (#1221, #1219): module sources come from
// node-lib/ via the native `__host_builtin_source`; the prelude modules that
// register themselves (fs, timers) are collected here and all of them are
// reached through `require`. This is the last prelude script before the
// globals are hidden: it captures and deletes the registries and the hidden
// entry points, and installs the CommonJS entry (`internal/modules/cjs/loader`)
// for the main module (`__node_run_main`, consumed by the launcher) or for
// `-e`/REPL (`require` and friends as globals); an ES module main gets neither.
// See docs/adr/2026-10-26-0854-node-host-mode.md.

(function () {
  "use strict";

  if (typeof __host_builtin_source === "undefined") return;

  var hostSource = __host_builtin_source;
  var mainKind = __host_main_kind();
  var exposeInternals = __host_expose_internals();
  delete globalThis.__host_builtin_source;
  delete globalThis.__host_main_kind;
  delete globalThis.__host_expose_internals;

  // Preludes that register a public module put it in one of these; both are
  // taken private here.
  var loopKey = Symbol.for("jsse.node.builtins");
  var registries = [globalThis.__node_builtins, globalThis[loopKey]];
  delete globalThis.__node_builtins;
  delete globalThis[loopKey];
  var registered = Object.create(null);
  registries.forEach(function (registry) {
    if (registry === undefined) return;
    Object.keys(registry).forEach(function (id) {
      registered[id] = registry[id];
    });
  });

  var indirectEval = eval;
  var proc = globalThis.process;
  var uncurryThis = Function.prototype.bind.bind(Function.prototype.call);
  var strSlice = uncurryThis(String.prototype.slice);

  var INTRINSICS = [
    "Array", "ArrayBuffer", "BigInt", "BigInt64Array", "BigUint64Array",
    "Boolean", "DataView", "Date", "Error", "EvalError", "Float32Array",
    "Float64Array", "Function", "Int16Array", "Int32Array", "Int8Array",
    "Map", "Number", "Object", "Promise", "RangeError", "ReferenceError",
    "RegExp", "Set", "String", "Symbol", "SyntaxError", "TypeError",
    "URIError", "Uint16Array", "Uint32Array", "Uint8Array", "AggregateError",
    "TypedArray",
    "Uint8ClampedArray", "WeakMap", "WeakRef", "WeakSet",
    "JSON", "Math", "Reflect", "Proxy",
  ];

  function pascal(key) {
    if (typeof key === "symbol") {
      var d = key.description;
      return d === undefined
        ? ""
        : "Symbol" + d.charAt(7).toUpperCase() + d.slice(8);
    }
    return key.charAt(0).toUpperCase() + key.slice(1);
  }

  // Node's `primordials` (lib/internal/per_context/primordials.js) naming:
  // statics are `ObjectKeys` (bound for Promise, whose statics need `this`),
  // prototype methods are uncurried `ArrayPrototypeJoin`, accessors are
  // `TypedArrayPrototypeGetLength`.
  // The intrinsic values are snapshotted here, before user code runs; the
  // uncurried wrappers are created on first use of each name.
  var table = Object.create(null);
  var resolved = Object.create(null);
  function snapshot(source, prefix, uncurry, bindTo) {
    Reflect.ownKeys(source).forEach(function (key) {
      if (key === "constructor" || key === "__proto__") return;
      var d = Object.getOwnPropertyDescriptor(source, key);
      var base = prefix + pascal(key);
      if ("get" in d || "set" in d) {
        if (d.get) table[prefix + "Get" + pascal(key)] = [d.get, true];
        if (d.set) table[prefix + "Set" + pascal(key)] = [d.set, true];
      } else {
        var value = d.value;
        if (bindTo && typeof value === "function") {
          value = Function.prototype.bind.call(value, bindTo);
        }
        table[base] = [value, uncurry && typeof value === "function"];
      }
    });
  }
  INTRINSICS.forEach(function (name) {
    var ctor = name === "TypedArray"
      ? Object.getPrototypeOf(Uint8Array)
      : globalThis[name];
    table[name] = [ctor, false];
    snapshot(ctor, name, false, name === "Promise" ? ctor : undefined);
    if (typeof ctor === "function" && ctor.prototype) {
      table[name + "Prototype"] = [ctor.prototype, false];
      snapshot(ctor.prototype, name + "Prototype", true);
    }
  });
  ["decodeURI", "decodeURIComponent", "encodeURI", "encodeURIComponent",
    "parseFloat", "parseInt", "isNaN", "isFinite"].forEach(function (name) {
    table[name] = [globalThis[name], false];
  });

  table.uncurryThis = [uncurryThis, false];
  table.globalThis = [globalThis, false];
  var stringIterator = uncurryThis(String.prototype[Symbol.iterator]);
  var arrayIterator = uncurryThis(Array.prototype[Symbol.iterator]);
  var stringNext = uncurryThis(
    Object.getPrototypeOf(""[Symbol.iterator]()).next
  );
  var arrayNext = uncurryThis(
    Object.getPrototypeOf([][Symbol.iterator]()).next
  );
  function SafeIterator(createIterator, iteratorNext) {
    return class {
      constructor(iterable) {
        this._iterator = createIterator(iterable);
      }
      next() {
        return iteratorNext(this._iterator);
      }
      [Symbol.iterator]() {
        return this;
      }
    };
  }
  table.SafeStringIterator = [SafeIterator(stringIterator, stringNext), false];
  table.SafeArrayIterator = [SafeIterator(arrayIterator, arrayNext), false];
  ["Map", "Set", "WeakMap", "WeakSet", "WeakRef"].forEach(function (name) {
    var Base = globalThis[name];
    table["Safe" + name] = [class extends Base {}, false];
  });

  var reflectApply = Reflect.apply;
  var arrayPush = Array.prototype.push;
  var arrayUnshift = Array.prototype.unshift;
  var regExpExec = uncurryThis(RegExp.prototype.exec);
  table.ArrayPrototypePushApply = [
    function (array, items) { return reflectApply(arrayPush, array, items); },
    false,
  ];
  table.ArrayPrototypeUnshiftApply = [
    function (array, items) { return reflectApply(arrayUnshift, array, items); },
    false,
  ];
  table.SafeStringPrototypeSearch = [
    function (string, regexp) {
      regexp.lastIndex = 0;
      var match = regExpExec(regexp, string);
      return match ? match.index : -1;
    },
    false,
  ];
  table.AsyncIteratorPrototype = [
    Object.getPrototypeOf(Object.getPrototypeOf(async function* () {}).prototype),
    false,
  ];
  if (table.ErrorCaptureStackTrace === undefined) {
    table.ErrorCaptureStackTrace = [function captureStackTrace() {}, false];
  }

  var primordials = new Proxy(Object.create(null), {
    get: function (_, name) {
      if (name in resolved) return resolved[name];
      var entry = table[name];
      if (entry === undefined) return undefined;
      return (resolved[name] = entry[1] ? uncurryThis(entry[0]) : entry[0]);
    },
    has: function (_, name) {
      return name in table;
    },
  });

  var cache = Object.create(null);
  function takeUtilHost(name) {
    var fn = globalThis[name];
    delete globalThis[name];
    return fn;
  }
  cache["internal/host/util"] = {
    exports: {
      kind: takeUtilHost("__host_util_kind"),
      promiseDetails: takeUtilHost("__host_util_promise_details"),
      proxyDetails: takeUtilHost("__host_util_proxy_details"),
      previewEntries: takeUtilHost("__host_util_preview_entries"),
      ownNonIndex: takeUtilHost("__host_util_own_non_index"),
      className: takeUtilHost("__host_util_class_name"),
      builtinSource: hostSource,
    },
  };
  cache["internal/host/buffer"] = {
    exports: { Buffer: globalThis.Buffer, atob: globalThis.atob, btoa: globalThis.btoa },
  };
  cache["internal/host/encoding"] = {
    exports: {
      TextEncoder: globalThis.TextEncoder,
      TextDecoder: globalThis.TextDecoder,
    },
  };

  function takeHost(name) {
    var fn = globalThis[name];
    delete globalThis[name];
    return fn;
  }
  cache["internal/url/idna"] = {
    exports: {
      toASCII: takeHost("__host_idna_to_ascii"),
      toUnicode: takeHost("__host_idna_to_unicode"),
    },
  };
  // The CommonJS loader names each module's source for stack frames (#1228).
  cache["internal/host/stack"] = {
    exports: {
      nameSource: takeHost("__host_stack_name_source"),
      throwSite: takeHost("__host_stack_throw_site"),
      evalSource: takeHost("__host_stack_eval_source"),
    },
  };
  cache["internal/host/vm"] = { exports: takeHost("__host_vm") };
  // The native `os` binding (#1222) is private to node-lib/os.js.
  var hostOs = globalThis.__host_os;
  delete globalThis.__host_os;
  hostOs.Buffer = globalThis.Buffer;
  cache["internal/host/os"] = { exports: hostOs };
  // The terminal and standard-stream floor (#1232), private to the stdio and
  // tty modules; without the unix floor, stdout and stderr go through
  // __host_write and stdin is empty.
  var hostTty = takeHost("__host_tty");
  if (hostTty === undefined) {
    var hostWrite = globalThis.__host_write;
    var utf8 = new TextDecoder();
    hostTty = {
      guessHandleType: function (fd) { return fd >= 0 && fd <= 2 ? "PIPE" : "INVALID"; },
      isTTY: function () { return false; },
      getWindowSize: function () {},
      setRawMode: function () { return { code: "ENOTTY", errno: -25 }; },
      write: function (fd, data) {
        return hostWrite(fd, typeof data === "string" ? data : utf8.decode(data));
      },
      stdinStart: function (cb) { Promise.resolve().then(function () { cb(null); }); },
      stdinStop: function () {},
      stdinRef: function () {},
    };
  }
  cache["internal/host/tty"] = { exports: hostTty };

  // The native process-metrics binding (#1227): the prelude's `process` already
  // holds it; `load` lets those methods reach per_thread.js on first use.
  var hostProcess = globalThis.__host_process;
  delete globalThis.__host_process;
  hostProcess.load = requireBuiltin;
  cache["internal/host/process"] = { exports: hostProcess };

  // The native `crypto` binding (#1226), plus the entropy floor and the global
  // Web Crypto object the module re-exports as `webcrypto`.
  var hostCrypto = globalThis.__host_crypto;
  delete globalThis.__host_crypto;
  hostCrypto.randomBytes = globalThis.__host_random_bytes;
  hostCrypto.webcrypto = globalThis.crypto;
  cache["internal/host/crypto"] = { exports: hostCrypto };

  // The native `zlib` stream table (#1239).
  var hostZlib = globalThis.__host_zlib;
  delete globalThis.__host_zlib;
  cache["internal/host/zlib"] = { exports: hostZlib };

  // The child process and signal floor (#1236), assembled by node-process-signals.js.
  var childKey = Symbol.for("jsse.node.child");
  var childBinding = globalThis[childKey];
  delete globalThis[childKey];
  if (childBinding !== undefined) {
    childBinding.load = function () {
      return requireBuiltin("internal/process/signals");
    };
    cache["internal/host/child_process"] = { exports: childBinding };
  }

  function requireBuiltin(id) {
    var cached = cache[id];
    if (cached !== undefined) return cached.exports;
    if (id in registered) return registered[id];
    var source = hostSource(id);
    if (source === undefined) {
      var codes = requireBuiltin("internal/errors").codes;
      throw new codes.ERR_UNKNOWN_BUILTIN_MODULE(id);
    }
    var fn = indirectEval(
      "(function (exports, require, module, process, primordials) {" +
        source +
        "\n})"
    );
    cache["internal/host/stack"].exports.nameSource(fn, "node:" + id, 0);
    var module = { exports: {}, id: id };
    cache[id] = module;
    try {
      fn(module.exports, requireBuiltin, module, proc, primordials);
    } catch (e) {
      delete cache[id];
      throw e;
    }
    return module.exports;
  }

  var requirable = Object.create(null);
  function canBeRequiredByUsers(id) {
    if (typeof id !== "string" || id.charAt(0) === "/" || id.charAt(0) === ".") {
      return false;
    }
    if (id in requirable) return true;
    var found =
      id in registered ||
      ((exposeInternals || strSlice(id, 0, 9) !== "internal/") &&
        hostSource(id) !== undefined);
    if (found) requirable[id] = true;
    return found;
  }

  // Node's exposeLazyInterfaces: an accessor that loads the module on first
  // touch and then settles into the plain writable, non-enumerable data
  // property the global has in Node.
  var LAZY_GLOBALS = [
    ["internal/event_target", "Event"],
    ["internal/event_target", "EventTarget"],
    ["internal/event_target", "CustomEvent"],
    ["internal/abort_controller", "AbortController"],
    ["internal/abort_controller", "AbortSignal"],
    ["internal/per_context/domexception", "DOMException"],
    ["perf_hooks", "performance"],
    ["perf_hooks", "PerformanceEntry"],
    ["perf_hooks", "PerformanceMark"],
    ["perf_hooks", "PerformanceMeasure"],
    ["perf_hooks", "PerformanceObserver"],
    ["perf_hooks", "PerformanceObserverEntryList"],
    ["perf_hooks", "PerformanceResourceTiming"],
  ];

  function defineLazyGlobal(id, name) {
    function settle(value) {
      Object.defineProperty(globalThis, name, {
        value: value,
        writable: true,
        enumerable: false,
        configurable: true,
      });
      return value;
    }
    Object.defineProperty(globalThis, name, {
      get: function () {
        var value = requireBuiltin(id)[name];
        if (value === undefined) {
          throw new Error("builtin " + id + " does not export " + name);
        }
        return settle(value);
      },
      set: settle,
      enumerable: false,
      configurable: true,
    });
  }
  LAZY_GLOBALS.forEach(function (entry) {
    defineLazyGlobal(entry[0], entry[1]);
  });

  // Buffer's inspect output, the part of util.inspect Buffer owns in Node.
  if (typeof Buffer === "function") {
    Buffer.prototype.inspect = Buffer.prototype[
      Symbol.for("nodejs.util.inspect.custom")
    ] = function inspect(depth, ctx, utilInspect) {
      return requireBuiltin("internal/buffer_inspect").inspect.call(
        this, depth, ctx, utilInspect
      );
    };
  }

  if (hostCrypto.webcrypto) {
    Object.defineProperty(hostCrypto.webcrypto, "subtle", {
      get: function () {
        return requireBuiltin("internal/crypto/webcrypto").subtle;
      },
      enumerable: true,
      configurable: true,
    });
  }

  // The fatal-error report (#1229) is built by node-lib/internal/process/fatal.js.
  var fatalKey = Symbol.for("jsse.node.fatal.handoff");
  var handoff = globalThis[fatalKey];
  delete globalThis[fatalKey];
  if (handoff !== undefined) {
    handoff.report = function (error, rejected) {
      return requireBuiltin("internal/process/fatal").report(error, rejected);
    };
    handoff.decorate = function (error) {
      return requireBuiltin("internal/process/fatal").decorate(error);
    };
    handoff.rejectionError = function (reason) {
      return requireBuiltin("internal/process/fatal").unhandledRejectionError(reason);
    };
  }

  // `console` writes through the embedded util module (Node's inspect/format).
  requireBuiltin("internal/console/node");
  requireBuiltin("internal/process/stdio");

  cache["internal/modules/builtins"] = {
    exports: { canBeRequiredByUsers: canBeRequiredByUsers, loadBuiltin: requireBuiltin },
  };

  // process.sourceMapsEnabled and setSourceMapsEnabled (#1220); the support
  // module loads on first use.
  Object.defineProperty(proc, "sourceMapsEnabled", {
    get: function () {
      return requireBuiltin("internal/source_map/support").getSourceMapsSupport().enabled;
    },
    enumerable: true,
    configurable: true,
  });
  Object.defineProperty(proc, "setSourceMapsEnabled", {
    value: function setSourceMapsEnabled(enabled) {
      requireBuiltin("internal/source_map/support").setSourceMapsEnabled(enabled);
    },
    writable: true,
    enumerable: true,
    configurable: true,
  });

  cache["internal/host/module"] = {
    exports: {
      tagFunction: takeHost("__host_tag_function"),
      esmRequire: takeHost("__host_esm_require"),
      esmFacade: takeHost("__host_esm_facade"),
      cjsLex: takeHost("__host_cjs_lex"),
    },
  };
  takeHost("__host_esm_install")(function (op, a, b, c) {
    return requireBuiltin("internal/modules/esm/host").host(op, a, b, c);
  });

  function cjsLoader() {
    return requireBuiltin("internal/modules/cjs/loader");
  }

  // A forked child opens the IPC channel it inherited before its main program
  // runs (#1237), as Node's setupChildProcessIpcChannel does.
  if (childBinding !== undefined && proc.env.NODE_CHANNEL_FD !== undefined) {
    var channelFd = parseInt(proc.env.NODE_CHANNEL_FD, 10);
    var channelMode = proc.env.NODE_CHANNEL_SERIALIZATION_MODE || "json";
    delete proc.env.NODE_CHANNEL_FD;
    delete proc.env.NODE_CHANNEL_SERIALIZATION_MODE;
    if (channelFd >= 0) requireBuiltin("child_process")._forkChild(channelFd, channelMode);
  }

  if (mainKind === "cjs") {
    Object.defineProperty(globalThis, "__node_run_main", {
      value: function () {
        cjsLoader().runMain();
      },
      writable: true,
      enumerable: false,
      configurable: true,
    });
  } else if (mainKind === "eval") {
    cjsLoader().setupEval();
  }

  // URL and URLSearchParams are non-enumerable data properties in Node. Here
  // they are accessors until first use, so launches that never touch them do
  // not parse the url sources; the first read or write turns them into data
  // properties.
  ["URL", "URLSearchParams"].forEach(function (name) {
    function settle(value) {
      Object.defineProperty(globalThis, name, {
        value: value,
        writable: true,
        enumerable: false,
        configurable: true,
      });
      return value;
    }
    Object.defineProperty(globalThis, name, {
      get: function () {
        return settle(requireBuiltin("internal/url")[name]);
      },
      set: function (value) {
        if (this === globalThis) settle(value);
        else if (Object(this) === this) {
          Object.defineProperty(this, name, {
            value: value,
            writable: true,
            enumerable: true,
            configurable: true,
          });
        }
      },
      enumerable: false,
      configurable: true,
    });
  });
})();
//# sourceURL=node:internal/bootstrap/realm
