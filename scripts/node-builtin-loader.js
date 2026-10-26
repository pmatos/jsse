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
  delete globalThis.__host_builtin_source;
  delete globalThis.__host_main_kind;

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
    "URIError", "Uint16Array", "Uint32Array", "Uint8Array",
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
    var ctor = globalThis[name];
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
  cache["internal/util/inspect"] = {
    exports: { inspect: globalThis.util.inspect },
  };

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
    if (id in requirable) return requirable[id];
    return (requirable[id] =
      id in registered ||
      (strSlice(id, 0, 9) !== "internal/" && hostSource(id) !== undefined));
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

  cache["internal/modules/builtins"] = {
    exports: { canBeRequiredByUsers: canBeRequiredByUsers, loadBuiltin: requireBuiltin },
  };

  function cjsLoader() {
    return requireBuiltin("internal/modules/cjs/loader");
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
})();
