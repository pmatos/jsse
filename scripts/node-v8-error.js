// V8-compatible Error API for Node host mode (issue #1228): the `stack`
// accessor, Error.captureStackTrace, Error.stackTraceLimit,
// Error.prepareStackTrace and CallSite objects. The engine records raw frames
// natively (src/interpreter/builtins/node_host_stack.rs); this script formats
// them on the first read of `stack`, as V8 does. Loaded first so every later
// prelude script and user code sees it. See
// docs/adr/2026-10-26-1000-node-host-v8-errors.md.
(function () {
  "use strict";

  var install = __host_stack_install;
  var captureFrames = __host_stack_capture;
  var framesOf = __host_stack_frames;
  var locate = __host_stack_locate;
  var arrowOf = __host_stack_arrow;
  delete globalThis.__host_stack_install;
  delete globalThis.__host_stack_capture;
  delete globalThis.__host_stack_frames;
  delete globalThis.__host_stack_locate;
  delete globalThis.__host_stack_arrow;

  var ErrorCtor = Error;
  var defineProperty = Object.defineProperty;
  var kIsNodeError = Symbol.for("kIsNodeError");
  var reflectApply = Reflect.apply;
  var stringCtor = String;
  var RECORD = 6;
  var CONSTRUCTOR = 1;
  var TOPLEVEL = 2;
  var ASYNC = 4;
  var COMBINATOR = 8;

  function endsWith(text, suffix) {
    return text.length >= suffix.length && text.slice(text.length - suffix.length) === suffix;
  }

  function describe(frames, index) {
    var base = index * RECORD;
    var combinator = (frames[base + 4] & COMBINATOR) !== 0;
    var place = combinator ? undefined : locate(frames[base + 2], frames[base + 3]);
    return {
      functionName: frames[base] === undefined ? null : frames[base],
      typeName: frames[base + 1] === undefined ? null : frames[base + 1],
      file: combinator ? null : place === undefined ? undefined : place[0],
      line: place === undefined ? null : place[1],
      column: place === undefined ? null : place[2],
      constructor: (frames[base + 4] & CONSTRUCTOR) !== 0,
      toplevel: (frames[base + 4] & TOPLEVEL) !== 0,
      async: (frames[base + 4] & ASYNC) !== 0,
      method: frames[base + 5] === undefined ? null : frames[base + 5],
      promiseIndex: combinator ? frames[base + 3] : null,
    };
  }

  class CallSite {
    #frame;

    constructor(frame) {
      this.#frame = frame;
    }

    getThis() {
      return undefined;
    }
    getTypeName() {
      return this.#frame.typeName;
    }
    getFunction() {
      return undefined;
    }
    getFunctionName() {
      return this.#frame.functionName;
    }
    getMethodName() {
      return this.#frame.method;
    }
    getFileName() {
      return this.#frame.file;
    }
    getScriptNameOrSourceURL() {
      return this.#frame.file;
    }
    getScriptHash() {
      return "";
    }
    getLineNumber() {
      return this.#frame.line;
    }
    getColumnNumber() {
      return this.#frame.column;
    }
    getEnclosingLineNumber() {
      return this.#frame.line;
    }
    getEnclosingColumnNumber() {
      return this.#frame.column;
    }
    getPosition() {
      return this.#frame.promiseIndex === null ? 0 : this.#frame.promiseIndex;
    }
    getEvalOrigin() {
      return undefined;
    }
    getPromiseIndex() {
      return this.#frame.promiseIndex;
    }
    isToplevel() {
      return this.#frame.toplevel;
    }
    isEval() {
      return false;
    }
    isNative() {
      return false;
    }
    isConstructor() {
      return this.#frame.constructor;
    }
    isAsync() {
      return this.#frame.async;
    }
    isPromiseAll() {
      return this.#frame.promiseIndex !== null && this.#frame.functionName === "all";
    }
    toString() {
      var text = this.#plain();
      return this.#frame.async ? "async " + text : text;
    }
    #plain() {
      var frame = this.#frame;
      if (frame.promiseIndex !== null) {
        return "Promise." + frame.functionName + " (index " + frame.promiseIndex + ")";
      }
      var where =
        (frame.file === undefined || frame.file === null
          ? "<anonymous>"
          : frame.file) +
        (frame.line === null ? "" : ":" + frame.line + ":" + frame.column);
      var name = frame.functionName;
      if (frame.constructor) {
        return "new " + (name === null ? "<anonymous>" : name) + " (" + where + ")";
      }
      if (!frame.toplevel) {
        var type = frame.typeName;
        if (name === null) return type + ".<anonymous> (" + where + ")";
        var accessor = name.lastIndexOf("get ", 0) === 0 || name.lastIndexOf("set ", 0) === 0;
        var qualified = name.indexOf(".") !== -1;
        var method = frame.method;
        var alias =
          method !== null && !accessor && name !== method && !endsWith(name, "." + method)
            ? " [as " + method + "]"
            : "";
        if (type !== null && !accessor && !qualified && name !== type) {
          name = type + "." + name;
        }
        return name + alias + " (" + where + ")";
      }
      return name === null ? where : name + " (" + where + ")";
    }
  }

  function errorHeader(object) {
    try {
      var name = object.name;
      name = name === undefined ? "Error" : stringCtor(name);
      var message = object.message;
      message = message === undefined ? "" : stringCtor(message);
      if (kIsNodeError in object) {
        return name + " [" + stringCtor(object.code) + "]: " + message;
      }
      if (name === "") return message;
      return message === "" ? name : name + ": " + message;
    } catch (e) {
      return "<error>";
    }
  }

  var formatting = false;

  function formatStack(object, frames) {
    var count = frames.length / RECORD;
    var prepare = ErrorCtor.prepareStackTrace;
    if (typeof prepare === "function" && !formatting) {
      var sites = [];
      for (var i = 0; i < count; i++) sites.push(new CallSite(describe(frames, i)));
      formatting = true;
      try {
        return reflectApply(prepare, ErrorCtor, [object, sites]);
      } finally {
        formatting = false;
      }
    }
    var text = errorHeader(object);
    for (var j = 0; j < count; j++) {
      text += "\n    at " + new CallSite(describe(frames, j)).toString();
    }
    return text;
  }

  function setStack(value) {
    defineProperty(this, "stack", {
      __proto__: null,
      value: value,
      writable: true,
      enumerable: false,
      configurable: true,
    });
  }

  function getStack() {
    var frames = framesOf(this);
    if (frames === undefined) return undefined;
    var arrow = arrowOf(this);
    var value = formatStack(this, frames);
    if (arrow !== undefined) value = arrow + value;
    setStack.call(this, value);
    return value;
  }

  function captureStackTrace(object, constructorOpt) {
    if (
      object === null ||
      (typeof object !== "object" && typeof object !== "function")
    ) {
      throw new TypeError("Invalid argument");
    }
    captureFrames(object, constructorOpt);
  }

  defineProperty(ErrorCtor, "captureStackTrace", {
    value: captureStackTrace,
    writable: true,
    enumerable: false,
    configurable: true,
  });
  defineProperty(ErrorCtor, "stackTraceLimit", {
    value: 10,
    writable: true,
    enumerable: true,
    configurable: true,
  });
  defineProperty(globalThis, "__node_stack_getter", {
    value: getStack,
    configurable: true,
  });
  install(ErrorCtor, getStack, setStack);
})();
