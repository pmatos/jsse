// Node event-loop semantics, part 1 (issue #1229): process.nextTick with its
// queue ahead of promise jobs, queueMicrotask, the `process` event methods and
// the exit / beforeExit / uncaughtException / unhandledRejection / warning
// lifecycle, on the `__host_loop_*` floor (builtins/node_host_loop.rs). The
// loop itself is src/interpreter/node_event_loop.rs. node-timers-shim.js
// (part 2) picks up the helpers this file leaves on a one-shot handoff key.
// A no-op unless the --node floor is present, so a stray evaluation elsewhere
// cannot disturb the default global environment. See
// docs/adr/2026-10-26-0854-node-host-mode.md.
(function () {
  "use strict";

  if (
    typeof __host_loop_install === "undefined" ||
    typeof globalThis.process !== "object" ||
    globalThis.process === null
  ) {
    return;
  }

  var hostInstall = __host_loop_install;
  var hostSetImmediate = __host_loop_set_immediate;
  var hostTickScheduled = __host_tick_scheduled;
  var hostRunMicrotasks = __host_run_microtasks;
  var hostCheckpoint = __host_checkpoint;
  var hostImmediateInfo = __host_immediate_info;
  var hostTimerRef = __host_timer_ref;
  var hostExit = __host_exit;
  delete globalThis.__host_loop_install;
  delete globalThis.__host_loop_set_immediate;
  delete globalThis.__host_tick_scheduled;
  delete globalThis.__host_run_microtasks;
  delete globalThis.__host_checkpoint;
  delete globalThis.__host_immediate_info;
  delete globalThis.__host_timer_ref;

  var proc = globalThis.process;
  var slice = Array.prototype.slice;
  var promiseThen = Promise.prototype.then;
  var resolved = Promise.resolve();
  var noop = function () {};

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
      shown = "'" + (v.length > 28 ? v.slice(0, 25) + "..." : v) + "'";
    } else if (typeof v === "bigint") {
      shown = v + "n";
    } else {
      shown = String(v);
    }
    return " Received type " + typeof v + " (" + shown + ")";
  }

  function invalidArgType(name, expected, value) {
    var e = new TypeError(
      'The "' + name + '" argument must be ' + expected + "." + describeReceived(value)
    );
    e.code = "ERR_INVALID_ARG_TYPE";
    return e;
  }

  function validateFunction(value, name) {
    if (typeof value !== "function") {
      throw invalidArgType(name, "of type function", value);
    }
  }

  // ---- process events -------------------------------------------------------
  // A minimal registry, installed as process's prototype the way Node puts
  // EventEmitter.prototype there, so the full `events` module can replace it.
  var kEvents = Symbol("events");
  var emitterProto = {};

  function listenersOf(self, type, create) {
    var table = self[kEvents];
    if (table === undefined) {
      if (!create) return undefined;
      table = Object.create(null);
      Object.defineProperty(self, kEvents, { value: table, enumerable: false });
    }
    var list = table[type];
    if (list === undefined && create) list = table[type] = [];
    return list;
  }

  function addListener(self, type, fn, once, prepend) {
    validateFunction(fn, "listener");
    var list = listenersOf(self, type, true);
    var entry = { fn: fn, once: once };
    if (prepend) list.unshift(entry);
    else list.push(entry);
    return self;
  }

  var emitterMethods = {
    on: function on(type, fn) {
      return addListener(this, type, fn, false, false);
    },
    addListener: function addListener_(type, fn) {
      return addListener(this, type, fn, false, false);
    },
    prependListener: function prependListener(type, fn) {
      return addListener(this, type, fn, false, true);
    },
    once: function once(type, fn) {
      return addListener(this, type, fn, true, false);
    },
    prependOnceListener: function prependOnceListener(type, fn) {
      return addListener(this, type, fn, true, true);
    },
    off: function off(type, fn) {
      var list = listenersOf(this, type, false);
      if (list === undefined) return this;
      for (var i = list.length - 1; i >= 0; i--) {
        if (list[i].fn === fn) {
          list.splice(i, 1);
          break;
        }
      }
      return this;
    },
    removeAllListeners: function removeAllListeners(type) {
      var table = this[kEvents];
      if (table === undefined) return this;
      if (type === undefined) {
        Object.keys(table).forEach(function (k) {
          delete table[k];
        });
      } else {
        delete table[type];
      }
      return this;
    },
    emit: function emit(type) {
      var list = listenersOf(this, type, false);
      var args = slice.call(arguments, 1);
      if (list === undefined || list.length === 0) {
        if (type === "error") throw args[0];
        return false;
      }
      var snapshot = list.slice();
      for (var i = 0; i < snapshot.length; i++) {
        if (snapshot[i].once) {
          var at = list.indexOf(snapshot[i]);
          if (at !== -1) list.splice(at, 1);
        }
        snapshot[i].fn.apply(this, args);
      }
      return true;
    },
    listeners: function listeners(type) {
      var list = listenersOf(this, type, false) || [];
      return list.map(function (e) {
        return e.fn;
      });
    },
    rawListeners: function rawListeners(type) {
      return emitterMethods.listeners.call(this, type);
    },
    listenerCount: function listenerCount(type) {
      var list = listenersOf(this, type, false);
      return list === undefined ? 0 : list.length;
    },
    eventNames: function eventNames() {
      var table = this[kEvents] || {};
      return Object.keys(table).filter(function (k) {
        return table[k].length > 0;
      });
    },
    setMaxListeners: function setMaxListeners() {
      return this;
    },
    getMaxListeners: function getMaxListeners() {
      return 10;
    },
  };
  emitterMethods.removeListener = emitterMethods.off;
  Object.keys(emitterMethods).forEach(function (name) {
    Object.defineProperty(emitterProto, name, {
      value: emitterMethods[name],
      writable: true,
      enumerable: false,
      configurable: true,
    });
  });
  ["on", "once", "emit"].forEach(function (name) {
    delete proc[name];
  });
  Object.setPrototypeOf(proc, emitterProto);
  Object.defineProperty(emitterProto, "constructor", {
    value: function process() {},
    writable: true,
    enumerable: false,
    configurable: true,
  });

  // ---- uncaught exceptions --------------------------------------------------
  var exiting = false;

  function isErrorLike(value) {
    return (
      typeof value === "object" &&
      value !== null &&
      Object.prototype.hasOwnProperty.call(value, "stack")
    );
  }

  function stderrWrite(text) {
    try {
      proc.stderr.write(text);
    } catch (e) {
      __host_write(2, text);
    }
  }

  var handoff = { report: undefined, rejectionError: undefined };

  function printUncaught(error, rejected) {
    var text;
    try {
      text = handoff.report(error, rejected);
    } catch (e) {
      try {
        text = isErrorLike(error) && typeof error.stack === "string"
          ? error.stack
          : String(error);
      } catch (ignored) {
        text = "Uncaught exception";
      }
    }
    stderrWrite(text + "\n\nNode.js " + proc.version + "\n");
  }

  function emitExit(code) {
    if (exiting) return;
    exiting = true;
    proc._exiting = true;
    proc.emit("exit", code);
  }

  function fatal(error, code, rejected) {
    if (code === 1) {
      try {
        proc.exitCode = 1;
        emitExit(1);
      } catch (ignored) {}
    }
    printUncaught(error, rejected);
    hostExit(code === 1 && proc.exitCode !== undefined ? proc.exitCode : code);
  }

  // `entry` is set for an error that ended the ES module entry point: Node
  // delivers it as a rejection after that many microtask turns (and reports a
  // non-error value with the loader's own header).
  function triggerUncaught(error, origin, entry) {
    if (entry > 0) {
      var turns = resolved;
      for (var i = 1; i < entry; i++) turns = promiseThen.call(turns, noop);
      promiseThen.call(turns, function () {
        triggerUncaught(error, origin, 0);
      });
      return;
    }
    var type = origin || "uncaughtException";
    try {
      proc.emit("uncaughtExceptionMonitor", error, type);
    } catch (ignored) {}
    if (proc.listenerCount("uncaughtException") === 0) {
      fatal(error, 1, entry === 0 ? "entry" : type === "unhandledRejection");
      return;
    }
    try {
      proc.emit("uncaughtException", error, type);
    } catch (inHandler) {
      fatal(inHandler, 7);
    }
  }

  // ---- nextTick -------------------------------------------------------------
  var tickQueue = [];
  var tickHead = 0;
  var tickScheduled = false;

  function nextTick(callback) {
    validateFunction(callback, "callback");
    if (proc._exiting) return;
    var args;
    if (arguments.length > 1) args = slice.call(arguments, 1);
    tickQueue.push({ callback: callback, args: args });
    if (!tickScheduled) {
      tickScheduled = true;
      hostTickScheduled(true);
    }
  }

  // A throwing callback escapes to the loop, which reports it and abandons
  // this checkpoint: the queue state is already consistent to resume from.
  function processTicks() {
    do {
      while (tickHead < tickQueue.length) {
        var tock = tickQueue[tickHead];
        tickQueue[tickHead++] = undefined;
        if (tock.args === undefined) tock.callback();
        else tock.callback.apply(undefined, tock.args);
      }
      tickQueue = [];
      tickHead = 0;
      hostRunMicrotasks();
    } while (tickQueue.length > 0);
    tickScheduled = false;
    hostTickScheduled(false);
  }

  function queueMicrotask(callback) {
    validateFunction(callback, "callback");
    promiseThen.call(resolved, function () {
      try {
        callback();
      } catch (e) {
        triggerUncaught(e);
      }
    });
  }

  // ---- unhandled rejections -------------------------------------------------
  function unhandledRejectionError(reason) {
    return handoff.rejectionError(reason);
  }

  function processRejections(list) {
    for (var i = 0; i + 1 < list.length; i += 2) {
      var promise = list[i];
      var reason = list[i + 1];
      var handled = false;
      try {
        handled = proc.emit("unhandledRejection", reason, promise);
      } catch (e) {
        triggerUncaught(e);
        continue;
      }
      if (!handled) {
        var error = isErrorLike(reason) ? reason : unhandledRejectionError(reason);
        triggerUncaught(error, "unhandledRejection");
      }
    }
  }

  // ---- lifecycle ------------------------------------------------------------
  function lifecycle(kind) {
    try {
      if (kind === "beforeExit") {
        proc.emit("beforeExit", proc.exitCode || 0);
      } else {
        if (kind === "unsettled") proc.exitCode = 13;
        emitExit(proc.exitCode || 0);
      }
    } catch (e) {
      triggerUncaught(e);
    }
  }

  var originalExit = proc.exit;
  var exitFn = function exit(code) {
    if (code !== undefined) proc.exitCode = code;
    try {
      emitExit(proc.exitCode || 0);
    } catch (e) {
      triggerUncaught(e);
    }
    originalExit.call(proc);
  };
  proc.exit = exitFn;
  proc._exiting = false;

  // ---- warnings -------------------------------------------------------------
  var warned = false;
  function emitWarning(warning, type, code) {
    var options = type !== null && typeof type === "object" ? type : {};
    if (typeof type === "string") options = { type: type, code: code };
    var name = options.type || "Warning";
    var error = warning;
    if (typeof warning === "string") {
      error = new Error(warning);
      error.name = name;
      if (options.code) error.code = options.code;
    } else if (!(warning instanceof Error)) {
      throw invalidArgType("warning", "of type string or an instance of Error", warning);
    }
    if (name === "DeprecationWarning" && proc.noDeprecation) return;
    nextTick(function () {
      proc.emit("warning", error);
    });
  }
  proc.emitWarning = emitWarning;
  proc.on("warning", function onWarning(warning) {
    var text =
      "(node:" + proc.pid + ") " + (warning.code ? "[" + warning.code + "] " : "") +
      warning.name + ": " + warning.message + "\n";
    if (!warned) {
      warned = true;
      text += "(Use `node --trace-warnings ...` to show where the warning was created)\n";
    }
    proc.stderr.write(text);
  });

  proc.nextTick = nextTick;
  Object.defineProperty(globalThis, "queueMicrotask", {
    value: queueMicrotask,
    writable: true,
    enumerable: true,
    configurable: true,
  });

  hostInstall(processTicks, triggerUncaught, processRejections, lifecycle);

  handoff.hostSetImmediate = hostSetImmediate;
  handoff.hostImmediateInfo = hostImmediateInfo;
  handoff.hostTimerRef = hostTimerRef;
  handoff.hostCheckpoint = hostCheckpoint;
  handoff.triggerUncaught = triggerUncaught;
  handoff.validateFunction = validateFunction;
  handoff.invalidArgType = invalidArgType;
  handoff.emitWarning = emitWarning;
  Object.defineProperty(globalThis, Symbol.for("jsse.node.loop.handoff"), {
    value: handoff,
    configurable: true,
  });
  Object.defineProperty(globalThis, Symbol.for("jsse.node.fatal.handoff"), {
    value: handoff,
    configurable: true,
  });
})();
//# sourceURL=node:internal/process/task_queues
