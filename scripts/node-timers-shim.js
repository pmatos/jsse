// Node event-loop semantics, part 2 (issue #1229): Timeout and Immediate
// objects (ref / unref / hasRef / refresh / Symbol.toPrimitive), the global
// setTimeout / setInterval / setImmediate family, and the `timers` and
// `timers/promises` modules. Timeouts are built on the interpreter's native
// one-shot timers; immediates live in a JS queue run in the loop's check
// phase. Needs the helpers node-event-loop-shim.js leaves on the handoff key.
// The two modules are published on globalThis[Symbol.for("jsse.node.builtins")]
// until the CommonJS loader (#1219) can serve `require('timers')`.
(function () {
  "use strict";

  var handoffKey = Symbol.for("jsse.node.loop.handoff");
  var handoff = globalThis[handoffKey];
  if (handoff === undefined) return;
  delete globalThis[handoffKey];

  var hostSetImmediate = handoff.hostSetImmediate;
  var hostImmediateInfo = handoff.hostImmediateInfo;
  var hostTimerRef = handoff.hostTimerRef;
  var hostCheckpoint = handoff.hostCheckpoint;
  var triggerUncaught = handoff.triggerUncaught;
  var validateFunction = handoff.validateFunction;
  var invalidArgType = handoff.invalidArgType;
  var emitWarning = handoff.emitWarning;

  var nativeSetTimeout = globalThis.setTimeout;
  var nativeClearTimeout = globalThis.clearTimeout;
  var slice = Array.prototype.slice;

  var TIMEOUT_MAX = 2147483647;
  var kRefed = Symbol("refed");
  var kHasPrimitive = Symbol("kHasPrimitive");
  var kAsyncId = Symbol("asyncId");
  var nextAsyncId = 1;
  var knownTimersById = Object.create(null);

  // ---- Timeout ----------------------------------------------------------------
  function arm(timer) {
    var id = nativeSetTimeout(function () {
      fire(timer);
    }, timer._idleTimeout);
    timer._native = id;
    if (!timer[kRefed]) hostTimerRef(id, false);
  }

  function release(timer) {
    if (timer[kHasPrimitive]) delete knownTimersById[timer[kAsyncId]];
  }

  function fire(timer) {
    timer._native = 0;
    var callback = timer._onTimeout;
    try {
      if (typeof callback === "function") {
        if (timer._timerArgs === undefined) callback.call(timer);
        else callback.apply(timer, timer._timerArgs);
      }
    } catch (e) {
      triggerUncaught(e);
    }
    if (timer._repeat !== null && !timer._destroyed) {
      if (timer._native === 0) arm(timer);
    } else if (!timer._destroyed && timer._native === 0) {
      timer._destroyed = true;
      release(timer);
    }
    hostCheckpoint();
  }

  function Timeout(callback, after, args, isRepeat, isRefed) {
    after *= 1;
    if (!(after >= 1 && after <= TIMEOUT_MAX)) {
      if (after > TIMEOUT_MAX) {
        emitWarning(
          after + " does not fit into a 32-bit signed integer.\nTimeout duration was set to 1.",
          "TimeoutOverflowWarning"
        );
      }
      after = 1;
    }
    this._idleTimeout = after;
    this._onTimeout = callback;
    this._timerArgs = args;
    this._repeat = isRepeat ? after : null;
    this._destroyed = false;
    Object.defineProperty(this, "_native", { value: 0, writable: true, enumerable: false });
    Object.defineProperty(this, kRefed, { value: !!isRefed, writable: true, enumerable: false });
    Object.defineProperty(this, kHasPrimitive, { value: false, writable: true, enumerable: false });
    Object.defineProperty(this, kAsyncId, { value: ++nextAsyncId, enumerable: false });
    arm(this);
  }

  function unenroll(timer) {
    if (timer._native !== 0) {
      nativeClearTimeout(timer._native);
      timer._native = 0;
    }
    timer._destroyed = true;
    release(timer);
  }

  function define(proto, name, fn) {
    Object.defineProperty(proto, name, {
      value: fn,
      writable: true,
      enumerable: false,
      configurable: true,
    });
  }

  define(Timeout.prototype, "refresh", function refresh() {
    if (this._native !== 0) {
      nativeClearTimeout(this._native);
      this._native = 0;
    }
    this._destroyed = false;
    arm(this);
    return this;
  });
  define(Timeout.prototype, "unref", function unref() {
    this[kRefed] = false;
    if (this._native !== 0) hostTimerRef(this._native, false);
    return this;
  });
  define(Timeout.prototype, "ref", function ref() {
    this[kRefed] = true;
    if (this._native !== 0) hostTimerRef(this._native, true);
    return this;
  });
  define(Timeout.prototype, "hasRef", function hasRef() {
    return this[kRefed];
  });
  define(Timeout.prototype, "close", function close() {
    clearTimeout(this);
    return this;
  });
  define(Timeout.prototype, Symbol.toPrimitive, function () {
    var id = this[kAsyncId];
    if (!this[kHasPrimitive]) {
      this[kHasPrimitive] = true;
      knownTimersById[id] = this;
    }
    return id;
  });

  function setTimeout(callback, after) {
    validateFunction(callback, "callback");
    var args;
    if (arguments.length > 2) args = slice.call(arguments, 2);
    return new Timeout(callback, after, args, false, true);
  }

  function setInterval(callback, repeat) {
    validateFunction(callback, "callback");
    var args;
    if (arguments.length > 2) args = slice.call(arguments, 2);
    return new Timeout(callback, repeat, args, true, true);
  }

  function clearTimeout(timer) {
    if (timer instanceof Timeout) {
      timer._onTimeout = null;
      unenroll(timer);
      return;
    }
    if (typeof timer === "number" || typeof timer === "string") {
      var known = knownTimersById[timer];
      if (known !== undefined) {
        known._onTimeout = null;
        unenroll(known);
      }
    }
  }

  function clearInterval(timer) {
    clearTimeout(timer);
  }

  // ---- Immediate --------------------------------------------------------------
  var immediateQueue = [];
  var immediateCount = 0;
  var immediateRefCount = 0;

  function Immediate(callback, args) {
    this._onImmediate = callback;
    this._argv = args;
    this._destroyed = false;
    Object.defineProperty(this, kRefed, { value: true, writable: true, enumerable: false });
    immediateCount++;
    immediateRefCount++;
    immediateQueue.push(this);
    hostImmediateInfo(immediateCount, immediateRefCount);
  }

  define(Immediate.prototype, "ref", function ref() {
    if (this[kRefed] === false) {
      this[kRefed] = true;
      if (!this._destroyed) {
        immediateRefCount++;
        hostImmediateInfo(immediateCount, immediateRefCount);
      }
    }
    return this;
  });
  define(Immediate.prototype, "unref", function unref() {
    if (this[kRefed] === true) {
      this[kRefed] = false;
      if (!this._destroyed) {
        immediateRefCount--;
        hostImmediateInfo(immediateCount, immediateRefCount);
      }
    }
    return this;
  });
  define(Immediate.prototype, "hasRef", function hasRef() {
    return !!this[kRefed];
  });

  function setImmediate(callback) {
    validateFunction(callback, "callback");
    var args;
    if (arguments.length > 1) args = slice.call(arguments, 1);
    return new Immediate(callback, args);
  }

  function clearImmediate(immediate) {
    if (!immediate || immediate._destroyed || !(immediate instanceof Immediate)) return;
    immediateCount--;
    immediate._destroyed = true;
    if (immediate[kRefed]) immediateRefCount--;
    immediate[kRefed] = null;
    immediate._onImmediate = null;
    hostImmediateInfo(immediateCount, immediateRefCount);
  }

  // Only immediates queued before the check phase began run in it; ones queued
  // by a callback wait for the next loop iteration.
  function processImmediate() {
    var queue = immediateQueue;
    immediateQueue = [];
    var ranOne = false;
    for (var i = 0; i < queue.length; i++) {
      var immediate = queue[i];
      if (immediate._destroyed) continue;
      if (ranOne) hostCheckpoint();
      ranOne = true;
      immediate._destroyed = true;
      immediateCount--;
      if (immediate[kRefed]) immediateRefCount--;
      immediate[kRefed] = null;
      hostImmediateInfo(immediateCount, immediateRefCount);
      var callback = immediate._onImmediate;
      var args = immediate._argv;
      try {
        if (args === undefined) callback.call(immediate);
        else callback.apply(immediate, args);
      } catch (e) {
        triggerUncaught(e);
      }
      immediate._onImmediate = null;
    }
  }
  hostSetImmediate(processImmediate);

  // ---- timers/promises --------------------------------------------------------
  function AbortError(message, options) {
    var e = new Error(message === undefined ? "The operation was aborted" : message);
    e.name = "AbortError";
    e.code = "ABORT_ERR";
    if (options && "cause" in options) e.cause = options.cause;
    return e;
  }

  function validateOptions(options) {
    if (options === undefined) return {};
    if (typeof options !== "object" || options === null) {
      throw invalidArgType("options", "of type object", options);
    }
    var signal = options.signal;
    if (
      signal !== undefined &&
      (typeof signal !== "object" || signal === null || !("aborted" in signal))
    ) {
      throw invalidArgType("options.signal", "an instance of AbortSignal", signal);
    }
    if (options.ref !== undefined && typeof options.ref !== "boolean") {
      throw invalidArgType("options.ref", "of type boolean", options.ref);
    }
    return options;
  }

  function settleOnTimer(make, delay, value, options) {
    var signal;
    try {
      options = validateOptions(options);
      signal = options.signal;
    } catch (e) {
      return Promise.reject(e);
    }
    if (signal && signal.aborted) {
      return Promise.reject(AbortError(undefined, { cause: signal.reason }));
    }
    return new Promise(function (resolve, reject) {
      var handle = make(
        function () {
          if (signal) signal.removeEventListener("abort", onAbort);
          resolve(value);
        },
        delay,
        options.ref !== false
      );
      function onAbort() {
        if (handle instanceof Timeout) clearTimeout(handle);
        else clearImmediate(handle);
        reject(AbortError(undefined, { cause: signal.reason }));
      }
      if (signal) signal.addEventListener("abort", onAbort, { once: true });
    });
  }

  function timeoutHandle(callback, delay, refed) {
    return new Timeout(callback, delay, undefined, false, refed);
  }

  function immediateHandle(callback, delay, refed) {
    var immediate = new Immediate(callback, undefined);
    if (!refed) immediate.unref();
    return immediate;
  }

  function promisesSetTimeout(delay, value, options) {
    return settleOnTimer(timeoutHandle, delay, value, options);
  }

  function promisesSetImmediate(value, options) {
    return settleOnTimer(immediateHandle, undefined, value, options);
  }

  async function* promisesSetInterval(after, value, options) {
    options = validateOptions(options);
    var signal = options.signal;
    if (signal && signal.aborted) {
      throw AbortError(undefined, { cause: signal.reason });
    }
    var interval;
    var onCancel;
    try {
      var notYielded = 0;
      var callback;
      interval = new Timeout(
        function () {
          notYielded++;
          if (callback) {
            callback();
            callback = undefined;
          }
        },
        after,
        undefined,
        true,
        options.ref !== false
      );
      if (signal) {
        onCancel = function () {
          clearTimeout(interval);
          if (callback) {
            callback(Promise.reject(AbortError(undefined, { cause: signal.reason })));
            callback = undefined;
          }
        };
        signal.addEventListener("abort", onCancel, { once: true });
      }
      while (!(signal && signal.aborted)) {
        if (notYielded === 0) {
          await new Promise(function (resolve) {
            callback = resolve;
          });
        }
        for (; notYielded > 0; notYielded--) yield value;
      }
      throw AbortError(undefined, { cause: signal && signal.reason });
    } finally {
      if (interval) clearTimeout(interval);
      if (signal && onCancel) signal.removeEventListener("abort", onCancel);
    }
  }

  var timersPromises = {
    setTimeout: promisesSetTimeout,
    setImmediate: promisesSetImmediate,
    setInterval: promisesSetInterval,
    scheduler: {
      wait: function wait(delay, options) {
        return promisesSetTimeout(delay, undefined, options);
      },
      yield: function () {
        return promisesSetImmediate();
      },
    },
  };

  Object.defineProperty(setTimeout, Symbol.for("nodejs.util.promisify.custom"), {
    value: promisesSetTimeout,
    enumerable: false,
    configurable: true,
  });
  Object.defineProperty(setImmediate, Symbol.for("nodejs.util.promisify.custom"), {
    value: promisesSetImmediate,
    enumerable: false,
    configurable: true,
  });

  var timersModule = {
    setTimeout: setTimeout,
    clearTimeout: clearTimeout,
    setInterval: setInterval,
    clearInterval: clearInterval,
    setImmediate: setImmediate,
    clearImmediate: clearImmediate,
    promises: timersPromises,
  };

  Object.keys(timersModule).forEach(function (name) {
    if (name === "promises") return;
    Object.defineProperty(globalThis, name, {
      value: timersModule[name],
      writable: true,
      enumerable: true,
      configurable: true,
    });
  });

  Object.defineProperty(globalThis, Symbol.for("jsse.node.builtins"), {
    value: { timers: timersModule, "timers/promises": timersPromises },
    configurable: true,
  });
})();
