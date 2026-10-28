// The WebAssembly JS API of Node host mode (#1378) over the native `__host_wasm` binding (a wasmi
// interpreter, node_host_wasm*.rs): objects wrap integer handles, value coercion (which may run user
// code) happens here. Messages follow V8; Compile/Link error text after the prefix is wasmi's.
// Not provided: Tag, Exception, JSTag, shared memories, *Streaming, v128 (docs/adr/2026-10-28-1200).
(function () {
  "use strict";
  var host = globalThis.__host_wasm;
  delete globalThis.__host_wasm;

  var defineProperty = Object.defineProperty;
  var getOwnPropertyDescriptor = Object.getOwnPropertyDescriptor;
  var freeze = Object.freeze;
  var create = Object.create;
  var fround = Math.fround;
  var trunc = Math.trunc;
  var asIntN = BigInt.asIntN;
  var isView = ArrayBuffer.isView;
  var DataViewC = DataView;
  var NumberFn = Number;
  var StringFn = String;
  var TypeErr = TypeError;
  var RangeErr = RangeError;
  var PromiseC = Promise;
  var MapC = Map;
  var WeakMapC = WeakMap;
  var mapGet = Map.prototype.get;
  var mapSet = Map.prototype.set;
  var wmGet = WeakMap.prototype.get;
  var wmSet = WeakMap.prototype.set;
  var apply = Reflect.apply;
  var arrayFrom = Array.from;
  var AB = ArrayBuffer;
  var SAB = SharedArrayBuffer;

  // The API entry point being run, for error message prefixes.
  var api = "WebAssembly";

  function inApi(name, fn) {
    var saved = api;
    api = name;
    try {
      return fn();
    } finally {
      api = saved;
    }
  }

  function fail(Ctor, message) {
    throw new Ctor(api + ": " + message);
  }

  function isObject(v) {
    return (typeof v === "object" && v !== null) || typeof v === "function";
  }

  function wasmError(name) {
    var C = function (message, options) {
      return Reflect.construct(Error, [message, options], new.target === undefined ? C : new.target);
    };
    Object.setPrototypeOf(C, Error);
    C.prototype = create(Error.prototype);
    defineProperty(C, "name", { value: name, configurable: true });
    defineProperty(C, "length", { value: 1, configurable: true });
    var own = { constructor: C, name: name, message: "" };
    for (var k in own) {
      defineProperty(C.prototype, k, { value: own[k], writable: true, configurable: true });
    }
    return C;
  }
  var CompileError = wasmError("CompileError");
  var LinkError = wasmError("LinkError");
  var RuntimeError = wasmError("RuntimeError");

  host.init(function (kind, message) {
    switch (kind) {
      case "compile":
        return new CompileError(api + ": " + message);
      case "link":
        return new LinkError(api + ": " + message);
      case "runtime":
        return new RuntimeError(message);
      case "range":
        return new RangeErr(message);
      default:
        return new TypeErr(message);
    }
  });

  // wrapper object -> [brand, id], and brand + id -> wrapper object (one object per handle)
  var brands = new WeakMapC();
  var wrappers = new MapC();
  function remember(obj, brand, id) {
    apply(wmSet, brands, [obj, [brand, id]]);
    apply(mapSet, wrappers, [brand + id, obj]);
    return obj;
  }
  function brandOf(obj) {
    var entry = isObject(obj) ? apply(wmGet, brands, [obj]) : undefined;
    return entry === undefined ? [undefined, -1] : entry;
  }
  function handleOf(obj, brand, message) {
    var entry = brandOf(obj);
    if (entry[0] !== brand) fail(TypeErr, message);
    return entry[1];
  }
  function linkHandle(obj, brand, message) {
    var entry = brandOf(obj);
    if (entry[0] !== brand) fail(LinkError, message);
    return entry[1];
  }
  function wrapped(brand, id, proto) {
    var found = apply(mapGet, wrappers, [brand + id]);
    return found !== undefined ? found : remember(create(proto), brand, id);
  }

  var TYPE_CHARS = {
    i32: "i",
    i64: "l",
    f32: "f",
    f64: "d",
    externref: "e",
    anyfunc: "r",
    funcref: "r",
  };

  function toWasm(c, v) {
    switch (c) {
      case "i":
        return v | 0;
      case "l":
        return asIntN(64, v);
      case "f":
        return fround(v);
      case "d":
        return +v;
      case "r":
        if (v === null) return null;
        var entry = brandOf(v);
        if (entry[0] !== "func") {
          fail(TypeErr, "function-typed object must be null (if nullable) or a Wasm function object");
        }
        return entry[1];
      default:
        return v;
    }
  }
  function fromWasm(c, v) {
    return c === "r" && v !== null ? funcWrapper(v, 0) : v;
  }
  function defaultValue(c) {
    return c === "l" ? 0n : c === "e" ? undefined : c === "r" ? null : 0;
  }

  function funcWrapper(id, index) {
    var found = apply(mapGet, wrappers, ["func" + id]);
    if (found !== undefined) return found;
    var sig = host.funcSig(id);
    var split = sig.indexOf(">");
    var params = sig.slice(0, split);
    var results = sig.slice(split + 1);
    var n = params.length;
    var f = (...given) => {
      var args = [];
      for (var i = 0; i < n; i++) args.push(toWasm(params[i], given[i]));
      var r = host.call(id, args);
      if (results.length === 0) return undefined;
      if (results.length === 1) return fromWasm(results[0], r[0]);
      for (var j = 0; j < r.length; j++) r[j] = fromWasm(results[j], r[j]);
      return r;
    };
    defineProperty(f, "name", { value: StringFn(index), configurable: true });
    defineProperty(f, "length", { value: n, configurable: true });
    return remember(f, "func", id);
  }

  // A JS callable as a wasm import of signature `sig`; returns the results as an array.
  function importThunk(fn, sig) {
    var split = sig.indexOf(">");
    var params = sig.slice(0, split);
    var results = sig.slice(split + 1);
    return function () {
      var args = [];
      for (var i = 0; i < params.length; i++) args.push(fromWasm(params[i], arguments[i]));
      var r = apply(fn, undefined, args);
      if (results.length === 0) return [];
      if (results.length === 1) return [toWasm(results[0], r)];
      var list = arrayFrom(r);
      if (list.length !== results.length) fail(TypeErr, "multi-return length mismatch");
      for (var j = 0; j < list.length; j++) list[j] = toWasm(results[j], list[j]);
      return list;
    };
  }

  function readLimit(desc, name, max) {
    var v = desc[name];
    if (v === undefined) return undefined;
    var n = NumberFn(v);
    if (n !== n || n === Infinity || n === -Infinity) {
      fail(TypeErr, "Property '" + name + "' must be convertible to a valid number");
    }
    n = trunc(n);
    if (n < 0) fail(TypeErr, "Property '" + name + "' must be non-negative");
    if (n > max) {
      fail(RangeErr, "Property '" + name + "': value " + n + " is above the upper bound " + max);
    }
    return n;
  }
  function limits(desc, max) {
    var initial = readLimit(desc, "initial", max);
    var maximum = readLimit(desc, "maximum", max);
    if (initial === undefined) fail(TypeErr, "Property 'initial' is required");
    if (maximum !== undefined && maximum < initial) {
      fail(RangeErr, "Property 'maximum': value " + maximum + " is below the lower bound " + initial);
    }
    return [initial, maximum];
  }
  function delta(v) {
    var n = NumberFn(v);
    if (n !== n || n === Infinity || n === -Infinity || n < 0 || n > 4294967295) {
      fail(TypeErr, "Argument 0 must be convertible to a valid number");
    }
    return trunc(n);
  }

  function memoryId(obj, name) {
    return inApi(name, function () {
      return handleOf(obj, "memory", "Receiver is not a WebAssembly.Memory");
    });
  }

  class Memory {
    constructor(descriptor) {
      var lim = inApi("WebAssembly.Memory()", function () {
        if (!isObject(descriptor)) fail(TypeErr, "Argument 0 must be a memory descriptor");
        var l = limits(descriptor, 65536);
        if (descriptor.shared) fail(TypeErr, "shared memories are not supported");
        return l;
      });
      remember(this, "memory", host.memNew(lim[0], lim[1]));
    }
    get buffer() {
      return host.memBuffer(memoryId(this, "WebAssembly.Memory.buffer"));
    }
    grow(by) {
      var id = memoryId(this, "WebAssembly.Memory.grow()");
      var n = inApi("WebAssembly.Memory.grow()", function () {
        return delta(by);
      });
      return host.memGrow(id, n);
    }
  }

  function refType(v) {
    var c = v === undefined ? undefined : TYPE_CHARS[StringFn(v)];
    return c === "r" || c === "e" ? c : undefined;
  }
  function tableId(obj, name) {
    return inApi(name, function () {
      return handleOf(obj, "table", "Receiver is not a WebAssembly.Table");
    });
  }
  function tableIndex(id, c, index, name) {
    var i = inApi(name, function () {
      return delta(index);
    });
    var len = host.tableLen(id);
    if (i >= len) {
      var kind = c === "e" ? "externref" : "funcref";
      throw new RangeErr(name + ": invalid address " + i + " in " + kind + " table of size " + len);
    }
    return i;
  }

  class Table {
    constructor(descriptor, value) {
      var made = inApi("WebAssembly.Table()", function () {
        if (!isObject(descriptor)) fail(TypeErr, "Argument 0 must be a table descriptor");
        var c = refType(descriptor.element);
        if (c === undefined) {
          fail(TypeErr, "Descriptor property 'element' must be a WebAssembly reference type");
        }
        var lim = limits(descriptor, 10000000);
        var init = value === undefined ? defaultValue(c) : toWasm(c, value);
        return host.tableNew(c, lim[0], lim[1], init);
      });
      remember(this, "table", made);
    }
    get length() {
      return host.tableLen(tableId(this, "WebAssembly.Table.length()"));
    }
    grow(by, value) {
      var id = tableId(this, "WebAssembly.Table.grow()");
      var c = host.tableType(id);
      var n = inApi("WebAssembly.Table.grow()", function () {
        return delta(by);
      });
      var init = value === undefined ? defaultValue(c) : toWasm(c, value);
      return host.tableGrow(id, n, init);
    }
    set(index, value) {
      var id = tableId(this, "WebAssembly.Table.set()");
      var c = host.tableType(id);
      var i = tableIndex(id, c, index, "WebAssembly.Table.set()");
      if (arguments.length > 1 && c === "r" && value !== null && brandOf(value)[0] !== "func") {
        throw new TypeErr(
          "WebAssembly.Table.set(): Argument 1 is invalid for table: function-typed object must be null (if nullable) or a Wasm function object"
        );
      }
      var v = arguments.length > 1 ? inApi("WebAssembly.Table.set()", function () {
        return toWasm(c, value);
      }) : defaultValue(c);
      host.tableSet(id, i, v);
    }
    get(index) {
      var id = tableId(this, "WebAssembly.Table.get()");
      var c = host.tableType(id);
      return fromWasm(c, host.tableGet(id, tableIndex(id, c, index, "WebAssembly.Table.get()")));
    }
  }

  function globalId(obj, name) {
    return inApi(name, function () {
      return handleOf(obj, "global", "Receiver is not a WebAssembly.Global");
    });
  }

  class Global {
    constructor(descriptor, value) {
      var made = inApi("WebAssembly.Global()", function () {
        if (!isObject(descriptor)) fail(TypeErr, "Argument 0 must be a global descriptor");
        var mutable = !!descriptor.mutable;
        var c = TYPE_CHARS[StringFn(descriptor.value)];
        if (c === undefined) {
          fail(TypeErr, "Descriptor property 'value' must be a WebAssembly type");
        }
        var v = value === undefined ? defaultValue(c) : toWasm(c, value);
        return host.globalNew(c, mutable, v);
      });
      remember(this, "global", made);
    }
    get value() {
      var id = globalId(this, "get WebAssembly.Global.value");
      return fromWasm(host.globalType(id)[0], host.globalGet(id));
    }
    set value(v) {
      var id = globalId(this, "set WebAssembly.Global.value");
      var t = host.globalType(id);
      if (!t[1]) fail(TypeErr, "Can't set the value of an immutable global.");
      host.globalSet(id, toWasm(t[0], v));
    }
    valueOf() {
      var id = globalId(this, "WebAssembly.Global.valueOf()");
      return fromWasm(host.globalType(id)[0], host.globalGet(id));
    }
  }

  function isBufferSource(v) {
    // V8 takes ArrayBuffers and typed arrays, not DataViews.
    return (isView(v) && !(v instanceof DataViewC)) || v instanceof AB || v instanceof SAB;
  }

  function resolveImports(modId, importObject) {
    var rows = host.moduleImports(modId);
    if (rows.length > 0 && !isObject(importObject)) {
      fail(TypeErr, "Imports argument must be present and must be an object");
    }
    var externs = [];
    for (var i = 0; i < rows.length; i++) {
      var row = rows[i];
      var ns = importObject[row[0]];
      var where = "Import #" + i + ' "' + row[0] + '"';
      if (!isObject(ns)) fail(TypeErr, where + ": module is not an object or function");
      var v = ns[row[1]];
      where += ' "' + row[1] + '"';
      if (row[2] === "function") {
        if (typeof v !== "function") fail(LinkError, where + ": function import requires a callable");
        var entry = brandOf(v);
        externs.push(entry[0] === "func" ? ["function", entry[1]] : ["js", importThunk(v, row[3])]);
      } else if (row[2] === "memory") {
        var memory = linkHandle(v, "memory", where + ": memory import must be a WebAssembly.Memory object");
        externs.push(["memory", memory]);
      } else if (row[2] === "table") {
        externs.push(["table", linkHandle(v, "table", where + ": table import requires a WebAssembly.Table")]);
      } else if (row[2] === "global") {
        var held = brandOf(v);
        var plain = row[3] === "l" ? typeof v === "bigint" : typeof v === "number";
        if (held[0] === "global") {
          externs.push(["global", held[1]]);
        } else if (plain && !row[4]) {
          externs.push(["global", host.globalNew(row[3], false, toWasm(row[3], v))]);
        } else {
          fail(LinkError, where + ": global import must be a number, valid Wasm reference, or WebAssembly.Global object");
        }
      } else {
        fail(LinkError, where + ": unsupported import kind");
      }
    }
    return externs;
  }

  function instantiateModule(modId, importObject) {
    var externs = resolveImports(modId, importObject);
    var instId = host.instantiate(modId, externs);
    var rows = host.moduleExports(modId);
    var handles = host.instanceExports(instId, modId);
    var exports = create(null);
    for (var i = 0; i < rows.length; i++) {
      var kind = handles[i][0];
      var id = handles[i][1];
      var value;
      if (kind === "function") value = funcWrapper(id, rows[i][2]);
      else if (kind === "memory") value = wrapped("memory", id, Memory.prototype);
      else if (kind === "table") value = wrapped("table", id, Table.prototype);
      else value = wrapped("global", id, Global.prototype);
      defineProperty(exports, rows[i][0], { value: value, enumerable: true });
    }
    return freeze(exports);
  }

  var instanceExports = new WeakMapC();

  function moduleId(module, name) {
    return inApi(name, function () {
      return handleOf(module, "module", "Argument 0 must be a WebAssembly.Module");
    });
  }

  class Module {
    constructor(bytes) {
      var id = inApi("WebAssembly.Module()", function () {
        if (!isBufferSource(bytes)) fail(TypeErr, "Argument 0 must be a buffer source");
        return host.compile(bytes);
      });
      remember(this, "module", id);
    }
    static imports(module) {
      return host.moduleImports(moduleId(module, "WebAssembly.Module.imports()")).map(function (r) {
        return { module: r[0], name: r[1], kind: r[2] };
      });
    }
    static exports(module) {
      return host.moduleExports(moduleId(module, "WebAssembly.Module.exports()")).map(function (r) {
        return { name: r[0], kind: r[1] };
      });
    }
    static customSections(module, name) {
      var id = moduleId(module, "WebAssembly.Module.customSections()");
      if (name === undefined) {
        throw new TypeErr("WebAssembly.Module.customSections(): Argument 1 is required");
      }
      return host.customSections(id, StringFn(name));
    }
  }

  class Instance {
    constructor(module, importObject) {
      var exports = inApi("WebAssembly.Instance()", function () {
        var id = handleOf(module, "module", "Argument 0 must be a WebAssembly.Module");
        if (importObject !== undefined && !isObject(importObject)) {
          fail(TypeErr, "Argument 1 must be an object");
        }
        return instantiateModule(id, importObject);
      });
      apply(wmSet, instanceExports, [this, exports]);
    }
    get exports() {
      var e = apply(wmGet, instanceExports, [this]);
      if (e === undefined) {
        throw new TypeErr("WebAssembly.Instance.exports(): Receiver is not a WebAssembly.Instance");
      }
      return e;
    }
  }

  function validate(bytes) {
    return inApi("WebAssembly.validate()", function () {
      if (!isBufferSource(bytes)) fail(TypeErr, "Argument 0 must be a buffer source");
      return host.validate(bytes);
    });
  }

  function compile(bytes) {
    return new PromiseC(function (resolve) {
      resolve(
        inApi("WebAssembly.compile()", function () {
          if (!isBufferSource(bytes)) fail(TypeErr, "Argument 0 must be a buffer source");
          return remember(create(Module.prototype), "module", host.compile(bytes));
        })
      );
    });
  }

  function instantiate(source, importObject) {
    return new PromiseC(function (resolve) {
      resolve(
        inApi("WebAssembly.instantiate()", function () {
          var held = brandOf(source);
          var isModule = held[0] === "module";
          if (!isModule && !isBufferSource(source)) {
            fail(TypeErr, "Argument 0 must be a buffer source or a WebAssembly.Module object");
          }
          if (importObject !== undefined && !isObject(importObject)) {
            fail(TypeErr, "Argument 1 must be an object");
          }
          var id = isModule ? held[1] : host.compile(source);
          var module = isModule ? source : remember(create(Module.prototype), "module", id);
          var instance = create(Instance.prototype);
          apply(wmSet, instanceExports, [instance, instantiateModule(id, importObject)]);
          return isModule ? instance : { module: module, instance: instance };
        })
      );
    });
  }

  // ---- surface: attributes as V8 has them (WebIDL members are enumerable) ----
  [
    [Module, "Module", []],
    [Instance, "Instance", ["exports"]],
    [Memory, "Memory", ["grow", "buffer"]],
    [Table, "Table", ["length", "grow", "set", "get"]],
    [Global, "Global", ["valueOf", "value"]],
  ].forEach(function (r) {
    r[2].forEach(function (n) {
      var d = getOwnPropertyDescriptor(r[0].prototype, n);
      d.enumerable = true;
      defineProperty(r[0].prototype, n, d);
    });
    defineProperty(r[0], "length", { value: 1, configurable: true });
    defineProperty(r[0].prototype, Symbol.toStringTag, {
      value: "WebAssembly." + r[1],
      configurable: true,
    });
  });

  var WebAssembly = {};
  defineProperty(instantiate, "length", { value: 1, configurable: true });
  [compile, validate, instantiate].forEach(function (fn) {
    defineProperty(WebAssembly, fn.name, {
      value: fn,
      writable: true,
      enumerable: true,
      configurable: true,
    });
  });
  [Module, Instance, Table, Memory, Global, CompileError, LinkError, RuntimeError].forEach(
    function (C) {
      defineProperty(WebAssembly, C.name, { value: C, writable: true, configurable: true });
    }
  );
  defineProperty(WebAssembly, Symbol.toStringTag, { value: "WebAssembly", configurable: true });
  defineProperty(globalThis, "WebAssembly", { value: WebAssembly, writable: true, configurable: true });
})();
