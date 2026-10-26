// Embedded builtin-module registry for `--node` (issue #1221). The module
// sources live in node-lib/ and are looked up by name through the native
// `__host_builtin_source` floor primitive (src/interpreter/builtins/
// node_host_builtins.rs); nothing is parsed until a module is first loaded.
// Each source is the body of `function (exports, require, module, process,
// primordials)`, as in Node's own bootstrap. The CommonJS loader (#1219)
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
      return "Symbol" + d.charAt(7).toUpperCase() + d.slice(8);
    }
    return key.charAt(0).toUpperCase() + key.slice(1);
  }

  // Node's `primordials` (lib/internal/per_context/primordials.js) naming:
  // statics are `ObjectKeys`, prototype methods are uncurried
  // `ArrayPrototypeJoin`, accessors are `TypedArrayPrototypeGetLength`.
  // Safe* collections and promise helpers are added when a port needs them.
  function buildPrimordials() {
    var p = Object.create(null);
    function copy(source, prefix, uncurry) {
      Reflect.ownKeys(source).forEach(function (key) {
        if (key === "constructor" || key === "__proto__") return;
        var d = Object.getOwnPropertyDescriptor(source, key);
        var name = prefix + pascal(key);
        if ("get" in d || "set" in d) {
          if (d.get) p[prefix + "Get" + pascal(key)] = uncurryThis(d.get);
          if (d.set) p[prefix + "Set" + pascal(key)] = uncurryThis(d.set);
        } else if (uncurry && typeof d.value === "function") {
          p[name] = uncurryThis(d.value);
        } else {
          p[name] = d.value;
        }
      });
    }
    INTRINSICS.forEach(function (name) {
      var ctor = globalThis[name];
      p[name] = ctor;
      copy(ctor, name, false);
      if (typeof ctor === "function" && ctor.prototype) {
        p[name + "Prototype"] = ctor.prototype;
        copy(ctor.prototype, name + "Prototype", true);
      }
    });
    ["decodeURI", "decodeURIComponent", "encodeURI", "encodeURIComponent",
      "parseFloat", "parseInt", "isNaN", "isFinite"].forEach(function (name) {
      p[name] = globalThis[name];
    });
    return p;
  }

  var primordials;
  var cache = Object.create(null);
  cache["internal/util/inspect"] = {
    exports: { inspect: globalThis.util.inspect },
  };

  function requireBuiltin(id) {
    var cached = cache[id];
    if (cached !== undefined) return cached.exports;
    var source = hostSource(id);
    if (source === undefined) {
      var codes = requireBuiltin("internal/errors").codes;
      throw new codes.ERR_UNKNOWN_BUILTIN_MODULE(id);
    }
    if (primordials === undefined) primordials = buildPrimordials();
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
    id = String(id);
    var name = id.slice(0, 5) === "node:" ? id.slice(5) : id;
    if (name.slice(0, 9) === "internal/") {
      var codes = requireBuiltin("internal/errors").codes;
      throw new codes.ERR_UNKNOWN_BUILTIN_MODULE(id);
    }
    return requireBuiltin(name);
  }

  Object.defineProperty(globalThis, "__node_builtin", {
    value: loadPublic,
    writable: true,
    enumerable: false,
    configurable: true,
  });
})();
