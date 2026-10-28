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
  delete globalThis.__host_stack_install;
  delete globalThis.__host_stack_capture;
  delete globalThis.__host_stack_frames;
  delete globalThis.__host_stack_locate;

  var ErrorCtor = Error;
  var defineProperty = Object.defineProperty;
  var reflectApply = Reflect.apply;
  var stringCtor = String;
  var RECORD = 5;
  var CONSTRUCTOR = 1;
  var TOPLEVEL = 2;

  function describe(frames, index) {
    var base = index * RECORD;
    var place = locate(frames[base + 2], frames[base + 3]);
    return {
      functionName: frames[base] === undefined ? null : frames[base],
      typeName: frames[base + 1] === undefined ? null : frames[base + 1],
      file: place === undefined ? undefined : place[0],
      line: place === undefined ? null : place[1],
      column: place === undefined ? null : place[2],
      constructor: (frames[base + 4] & CONSTRUCTOR) !== 0,
      toplevel: (frames[base + 4] & TOPLEVEL) !== 0,
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
      var frame = this.#frame;
      if (frame.toplevel || frame.constructor) return null;
      var name = frame.functionName;
      return name === null ? null : name.slice(name.lastIndexOf(".") + 1);
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
      return 0;
    }
    getEvalOrigin() {
      return undefined;
    }
    getPromiseIndex() {
      return null;
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
      return false;
    }
    isPromiseAll() {
      return false;
    }
    toString() {
      var frame = this.#frame;
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
        if (type !== null && !accessor && !qualified && name.lastIndexOf(type, 0) !== 0) {
          name = type + "." + name;
        }
        return name + " (" + where + ")";
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
    var value = formatStack(this, frames);
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
