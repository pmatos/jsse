// Builtin-module registry for `--node` (#1221): module sources come from
// node-lib/ via the native `__host_builtin_source`. The CommonJS loader (#1219)
// should capture `__node_builtin` and then delete it from the global object.
// See docs/adr/2026-10-26-0854-node-host-mode.md.

(function () {
  "use strict";

  if (typeof __host_builtin_source === "undefined") return;

  var hostSource = __host_builtin_source;
  delete globalThis.__host_builtin_source;

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

  function takeHost(name) {
    var fn = globalThis[name];
    delete globalThis[name];
    return fn;
  }
  cache["buffer"] = { exports: { Buffer: globalThis.Buffer } };
  cache["internal/url/idna"] = {
    exports: {
      toASCII: takeHost("__host_idna_to_ascii"),
      toUnicode: takeHost("__host_idna_to_unicode"),
    },
  };

  function requireBuiltin(id) {
    var cached = cache[id];
    if (cached !== undefined) return cached.exports;
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

  function loadPublic(id) {
    if (typeof id !== "string") {
      var codes = requireBuiltin("internal/errors").codes;
      throw new codes.ERR_INVALID_ARG_TYPE("id", "string", id);
    }
    var name = strSlice(id, 0, 5) === "node:" ? strSlice(id, 5) : id;
    if (strSlice(name, 0, 9) === "internal/") {
      codes = requireBuiltin("internal/errors").codes;
      throw new codes.ERR_UNKNOWN_BUILTIN_MODULE(id);
    }
    return requireBuiltin(name);
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

  Object.defineProperty(globalThis, "__node_builtin", {
    value: loadPublic,
    writable: true,
    enumerable: false,
    configurable: true,
  });

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
      set: settle,
      enumerable: false,
      configurable: true,
    });
  });
})();
