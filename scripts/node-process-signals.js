// Node host-compat process identity and signals (issue #1236): a real
// process.pid, process.kill and process.on('SIGINT' ...), plus the dispatcher
// that native host events (child processes, signals) are delivered to. Loaded
// before node-builtin-loader.js, which hands the binding to the lazily loaded
// `child_process` and `internal/process/signals` modules. A no-op without the
// floor (plain jsse, non-unix). See docs/adr/2026-10-26-2300-node-host-child-process.md.

(function () {
  "use strict";

  if (
    typeof __host_cp_start === "undefined" ||
    typeof globalThis.process !== "object" ||
    globalThis.process === null
  ) {
    return;
  }

  function take(name) {
    var fn = globalThis[name];
    delete globalThis[name];
    return fn;
  }

  var proc = globalThis.process;
  var childHandler;
  var watchHandler;
  var binding = {
    kill: take("__host_kill"),
    abort: take("__host_abort"),
    signalWatch: take("__host_signal_watch"),
    spawn: take("__host_cp_spawn"),
    spawnSync: take("__host_cp_spawn_sync"),
    read: take("__host_cp_read"),
    write: take("__host_cp_write"),
    shutdown: take("__host_cp_shutdown"),
    close: take("__host_cp_close"),
    release: take("__host_cp_release"),
    adopt: take("__host_cp_adopt"),
    ipcRead: take("__host_cp_ipc_read"),
    ipcWrite: take("__host_cp_ipc_write"),
    killChild: take("__host_cp_kill"),
    loopRef: take("__host_loop_ref"),
    loopDefer: take("__host_loop_defer"),
    watchAdd: take("__host_watch_add"),
    watchRemove: take("__host_watch_remove"),
    onWatch: function (handler) {
      watchHandler = handler;
    },
    onEvent: function (handler) {
      childHandler = handler;
    },
  };
  var getpid = take("__host_getpid");

  if (proc.pid === 1) proc.pid = getpid();

  take("__host_cp_start")(function dispatch(kind, id, x, y) {
    if (kind === 5) binding.load().emitSignal(proc, x);
    else if (kind === 6 && watchHandler !== undefined) watchHandler(id, x, y);
    else if (childHandler !== undefined) childHandler(kind, id, x, y);
  });

  function define(name, value) {
    Object.defineProperty(proc, name, {
      value: value,
      writable: true,
      enumerable: false,
      configurable: true,
    });
  }

  define("kill", function kill(pid, sig) {
    return binding.load().kill(pid, sig);
  });

  define("abort", function abort() {
    binding.abort();
  });

  function isSignalEvent(type) {
    return typeof type === "string" && type.slice(0, 3) === "SIG";
  }

  ["on", "addListener", "once", "prependListener", "prependOnceListener"].forEach(
    function (name) {
      define(name, function (type, listener) {
        var emitter = Object.getPrototypeOf(proc)[name];
        emitter.apply(this, arguments);
        if (isSignalEvent(type)) {
          try {
            binding.load().syncWatch(this, type);
          } catch (e) {
            this.removeListener(type, listener);
            throw e;
          }
        }
        return this;
      });
    }
  );

  ["off", "removeListener", "removeAllListeners"].forEach(function (name) {
    define(name, function (type) {
      var emitter = Object.getPrototypeOf(proc)[name];
      emitter.apply(this, arguments);
      if (isSignalEvent(type)) binding.load().syncWatch(this, type);
      else if (type === undefined && name === "removeAllListeners") {
        binding.load().syncAll(this);
      }
      return this;
    });
  });

  globalThis[Symbol.for("jsse.node.child")] = binding;
})();
