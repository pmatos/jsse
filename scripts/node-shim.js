// Shared Node host-compat prelude for jsse library-test bundles.
//
// This shim is prepended (via scripts/run-library-tests.sh) to an esbuild
// bundle so that a real-world npm library's own test runner — written against
// Node globals — can execute on jsse. It is a pure-JS shim: nothing here is
// baked into jsse's default global object, so test262 is unaffected.
//
// The readable-output layer (process, the full console method set, and the
// util.format / util.inspect core they share) is built on top of the flag-gated
// Rust "syscall floor" (issue #229): __host_write (byte-accurate fd I/O),
// __host_hrtime (monotonic clock), and __host_exit (real process exit). The
// harness runs jsse with `--node` so those primitives exist; when they are
// absent (jsse without --node) each surface degrades to a pure-JS fallback.
//
// Everything below is skipped on real Node, where `process`, the full
// `console`, and `require('util')` already exist. That inertness is what lets
// `run-library-tests.sh --node` run the exact same bundle against Node as a
// reference oracle.

(function () {
  "use strict";

  // On Node, `process.versions.node` is set; the whole shim is a no-op there.
  var onNode =
    typeof process !== "undefined" &&
    !!(process.versions && process.versions.node);
  if (onNode) return;

  // The syscall floor (issue #229), present only under jsse `--node`.
  var hostWrite = typeof __host_write !== "undefined" ? __host_write : null;
  var hostHrtime = typeof __host_hrtime !== "undefined" ? __host_hrtime : null;
  var hostExit = typeof __host_exit !== "undefined" ? __host_exit : null;
  var fallbackConsoleLog = console.log;

  var NS_PER_SEC = 1000000000;

  // ---- util.inspect (best-effort) ------------------------------------------
  //
  // A readable, Node-flavoured rendering of arbitrary values for console.dir,
  // the %o/%O format specifiers, and console.log of non-strings. It is
  // deliberately NOT byte-compatible with Node's util.inspect (colour
  // heuristics, `<ref *N>` back-references, hidden keys, getters, Map/Set
  // internals — a bottomless pit); it only needs to be correct on depth,
  // cycles, and the common types.
  function quoteString(s) {
    return (
      "'" +
      String(s)
        .replace(/\\/g, "\\\\")
        .replace(/'/g, "\\'")
        .replace(/\n/g, "\\n") +
      "'"
    );
  }

  function isIdentifierKey(k) {
    return /^[A-Za-z_$][A-Za-z0-9_$]*$/.test(k);
  }

  // Capture uncurried intrinsics before bundled library code runs. Node's
  // formatter reads built-in internal slots rather than user-overridable
  // prototype methods.
  var functionCall = Function.prototype.call;
  var dateGetTime = functionCall.bind(Date.prototype.getTime);
  var dateToISOString = functionCall.bind(Date.prototype.toISOString);
  var numberValueOf = functionCall.bind(Number.prototype.valueOf);
  var stringValueOf = functionCall.bind(String.prototype.valueOf);
  var booleanValueOf = functionCall.bind(Boolean.prototype.valueOf);
  var bigintValueOf = functionCall.bind(BigInt.prototype.valueOf);
  var symbolToString = functionCall.bind(Symbol.prototype.toString);

  function inspect(value, opts) {
    opts = opts || {};
    var maxDepth = typeof opts.depth === "number" ? opts.depth : 2;
    var seen = [];

    function render(v, depth) {
      var t = typeof v;
      if (v === null) return "null";
      if (t === "undefined") return "undefined";
      if (t === "string") return quoteString(v);
      if (t === "number") return Object.is(v, -0) ? "-0" : String(v);
      if (t === "bigint") return String(v) + "n";
      if (t === "boolean") return String(v);
      if (t === "symbol") return v.toString();
      if (t === "function") {
        return "[Function" + (v.name ? ": " + v.name : " (anonymous)") + "]";
      }

      // Objects.
      if (seen.indexOf(v) !== -1) return "[Circular *1]";
      if (v instanceof Error) {
        return v.stack ? String(v.stack) : String(v.name) + ": " + String(v.message);
      }
      if (v instanceof RegExp) return String(v);
      if (v instanceof Date) {
        return isNaN(dateGetTime(v)) ? "Invalid Date" : dateToISOString(v);
      }
      if (v instanceof Number) {
        return "[Number: " + render(numberValueOf(v), depth) + "]";
      }
      if (v instanceof String) {
        return "[String: " + render(stringValueOf(v), depth) + "]";
      }
      if (v instanceof Boolean) {
        return "[Boolean: " + render(booleanValueOf(v), depth) + "]";
      }
      if (v instanceof BigInt) {
        return "[BigInt: " + render(bigintValueOf(v), depth) + "]";
      }
      if (v instanceof Symbol) return "[Symbol: " + symbolToString(v) + "]";

      if (depth < 0) return Array.isArray(v) ? "[Array]" : "[Object]";

      seen.push(v);
      var out;
      try {
        if (Array.isArray(v)) {
          var items = [];
          for (var i = 0; i < v.length; i++) items.push(renderMember(v, i, depth));
          out = items.length ? "[ " + items.join(", ") + " ]" : "[]";
        } else {
          var keys = Object.keys(v);
          var parts = [];
          for (var j = 0; j < keys.length; j++) {
            var k = keys[j];
            var label = isIdentifierKey(k) ? k : quoteString(k);
            parts.push(label + ": " + renderMember(v, k, depth));
          }
          var ctorName = constructorName(v);
          out = parts.length
            ? ctorName + "{ " + parts.join(", ") + " }"
            : ctorName + "{}";
        }
      } finally {
        seen.pop();
      }
      return out;
    }

    // Render one own property/element WITHOUT invoking accessors — Node's
    // util.inspect shows [Getter]/[Setter] rather than calling the getter, so a
    // throwing or side-effecting accessor (object property or array element)
    // cannot make a diagnostic print throw/mutate under jsse where it would not
    // under Node.
    function renderMember(container, key, depth) {
      var desc = Object.getOwnPropertyDescriptor(container, key);
      if (desc && (desc.get || desc.set)) {
        return desc.get ? (desc.set ? "[Getter/Setter]" : "[Getter]") : "[Setter]";
      }
      return render(desc ? desc.value : container[key], depth - 1);
    }

    // Derive the "ClassName " prefix without a plain `v.constructor` get, which
    // would invoke an accessor `constructor` or a Proxy get-trap — Node reads
    // constructor metadata via the prototype chain, not by calling a getter. Use
    // data descriptors only, and treat any exotic-trap throw as "no prefix".
    function constructorName(v) {
      try {
        var ctor;
        var own = Object.getOwnPropertyDescriptor(v, "constructor");
        if (own) {
          if (!own.get && !own.set) ctor = own.value;
        } else {
          var proto = Object.getPrototypeOf(v);
          var pd = proto
            ? Object.getOwnPropertyDescriptor(proto, "constructor")
            : null;
          if (pd && !pd.get && !pd.set) ctor = pd.value;
        }
        return ctor && ctor.name && ctor.name !== "Object" ? ctor.name + " " : "";
      } catch (e) {
        return "";
      }
    }

    return render(value, maxDepth);
  }

  // ---- util.format ----------------------------------------------------------
  //
  // Node's printf-style formatter. The %s %d %i %f %j %c %% specifiers are
  // deterministic and matched exactly; %o/%O defer to the best-effort inspect.
  var builtInObjectNames = (function () {
    var names = Object.create(null);
    var globals = Object.getOwnPropertyNames(globalThis);
    for (var i = 0; i < globals.length; i++) {
      var name = globals[i];
      if (/^[A-Z][a-zA-Z0-9]+$/.test(name)) names[name] = true;
    }
    return names;
  })();
  var objectHasOwnProperty = Object.prototype.hasOwnProperty;
  var symbolToPrimitive = Symbol.toPrimitive;

  function hasOwnProperty(value, key) {
    return objectHasOwnProperty.call(value, key);
  }

  function returnFalse() {
    return false;
  }

  // Match Node's hasBuiltInToString classification. A bundled library's
  // prototype method is user-defined even when inherited, while coercion hooks
  // owned by a built-in prototype route through inspect.
  function hasBuiltInToString(value) {
    var hasOwnToString = hasOwnProperty;
    var hasOwnToPrimitive = hasOwnProperty;

    if (typeof value.toString !== "function") {
      if (typeof value[symbolToPrimitive] !== "function") return true;
      if (hasOwnProperty(value, symbolToPrimitive)) return false;
      hasOwnToString = returnFalse;
    } else if (hasOwnProperty(value, "toString")) {
      return false;
    } else if (typeof value[symbolToPrimitive] !== "function") {
      hasOwnToPrimitive = returnFalse;
    } else if (hasOwnProperty(value, symbolToPrimitive)) {
      return false;
    }

    var pointer = value;
    do {
      pointer = Object.getPrototypeOf(pointer);
    } while (
      pointer !== null &&
      !hasOwnToString(pointer, "toString") &&
      !hasOwnToPrimitive(pointer, symbolToPrimitive)
    );

    // A callable hook visible through a Proxy get trap may not have an owner in
    // the reported prototype chain. Node can unwrap proxies internally; the
    // pure-JS shim cannot, so treat that hook as user-defined.
    if (pointer === null) return false;

    var descriptor = Object.getOwnPropertyDescriptor(pointer, "constructor");
    return (
      descriptor !== undefined &&
      typeof descriptor.value === "function" &&
      builtInObjectNames[descriptor.value.name] === true
    );
  }

  function convS(v) {
    var t = typeof v;
    if (t === "string") return v;
    if (t === "bigint") return String(v) + "n";
    if (t === "number") return Object.is(v, -0) ? "-0" : String(v);
    if (v === null) return "null";
    if (t === "undefined") return "undefined";
    if (t === "boolean") return String(v);
    if (t === "symbol") return v.toString();
    if (t === "function") return inspect(v, { depth: 0 });
    return hasBuiltInToString(v) ? inspect(v, { depth: 0 }) : String(v);
  }

  function convD(v) {
    var t = typeof v;
    if (t === "bigint") return String(v) + "n";
    if (t === "symbol") return "NaN";
    return String(Number(v));
  }

  function convI(v) {
    var t = typeof v;
    if (t === "bigint") return String(v) + "n";
    if (t === "symbol") return "NaN";
    return String(parseInt(v, 10));
  }

  function convF(v) {
    if (typeof v === "symbol") return "NaN";
    return String(parseFloat(v));
  }

  function convJ(v) {
    try {
      var s = JSON.stringify(v);
      return s === undefined ? "undefined" : s;
    } catch (e) {
      // Node's %j suppresses ONLY circular-structure failures (returning
      // "[Circular]") and re-throws everything else — BigInt, and user
      // toJSON/getter exceptions. jsse's circular error is
      // "Converting circular structure to JSON"; its BigInt/toJSON errors do not
      // mention "circular", so matching the message is safe here (the shim is
      // inert on Node, so this only ever sees jsse's error text).
      if (e && /circular/i.test(String(e.message))) return "[Circular]";
      throw e;
    }
  }

  function format() {
    var args = arguments;
    var first = args[0];
    if (typeof first !== "string") {
      // No format string: inspect every argument, join with a space.
      var pieces = [];
      for (var i = 0; i < args.length; i++) {
        pieces.push(typeof args[i] === "string" ? args[i] : inspect(args[i]));
      }
      return pieces.join(" ");
    }
    // A lone string argument is returned verbatim — Node performs no specifier
    // substitution unless there is at least one argument to format, so e.g.
    // format("%%") is "%%" and format("%s") is "%s", but format("%%", x)
    // substitutes.
    if (args.length === 1) return first;

    var out = "";
    var lastPos = 0;
    var argIndex = 1;
    var f = first;
    var n = f.length;
    for (var p = 0; p < n - 1; p++) {
      if (f.charCodeAt(p) !== 37 /* % */) continue;
      var next = f.charCodeAt(p + 1);
      if (next === 37 /* %% */) {
        out += f.slice(lastPos, p) + "%";
        lastPos = p + 2;
        p++;
        continue;
      }
      // Specifiers that consume an argument only fire while one remains.
      var repl = null;
      if (argIndex < args.length) {
        switch (next) {
          case 115: repl = convS(args[argIndex++]); break; // s
          case 100: repl = convD(args[argIndex++]); break; // d
          case 105: repl = convI(args[argIndex++]); break; // i
          case 102: repl = convF(args[argIndex++]); break; // f
          case 106: repl = convJ(args[argIndex++]); break; // j
          case 111: repl = inspect(args[argIndex++], { depth: 4 }); break; // o
          case 79: repl = inspect(args[argIndex++], {}); break; // O
          case 99: argIndex++; repl = ""; break; // c (CSS ignored)
        }
      }
      if (repl !== null) {
        out += f.slice(lastPos, p) + repl;
        lastPos = p + 2;
        p++;
      }
    }
    out += f.slice(lastPos);

    // Trailing arguments beyond the specifiers are appended, space-separated.
    for (; argIndex < args.length; argIndex++) {
      var extra = args[argIndex];
      out += " " + (typeof extra === "string" ? extra : inspect(extra));
    }
    return out;
  }

  globalThis.util = {
    format: format,
    formatWithOptions: function (opts, first) {
      return format.apply(null, Array.prototype.slice.call(arguments, 1));
    },
    inspect: inspect,
  };

  // ---- process --------------------------------------------------------------
  function makeStream(fd) {
    if (hostWrite) {
      return {
        fd: fd,
        isTTY: false,
        write: function (chunk, encodingOrCb, cb) {
          hostWrite(fd, String(chunk));
          var callback = typeof encodingOrCb === "function" ? encodingOrCb : cb;
          if (typeof callback === "function") callback();
          return true;
        },
        _flush: function () {},
      };
    }
    // Fallback: jsse without the syscall floor only exposes newline-appending
    // console.log, so accumulate partial writes and emit one line at a time.
    // Use the original native log because this shim replaces console.log below.
    var buf = "";
    return {
      fd: fd,
      isTTY: false,
      write: function (chunk, encodingOrCb, cb) {
        buf += String(chunk);
        var idx;
        while ((idx = buf.indexOf("\n")) !== -1) {
          fallbackConsoleLog.call(console, buf.slice(0, idx));
          buf = buf.slice(idx + 1);
        }
        var callback = typeof encodingOrCb === "function" ? encodingOrCb : cb;
        if (typeof callback === "function") callback();
        return true;
      },
      _flush: function () {
        if (buf.length) {
          fallbackConsoleLog.call(console, buf);
          buf = "";
        }
      },
    };
  }

  var stdout = makeStream(1);
  var stderr = makeStream(2);
  var hrtimeFn;

  function makeHrtime() {
    var hr;
    if (hostHrtime) {
      hr = function (prev) {
        var now = hostHrtime(); // BigInt nanoseconds, monotonic
        if (prev) {
          var prevNs =
            BigInt(prev[0]) * BigInt(NS_PER_SEC) + BigInt(prev[1]);
          var delta = now - prevNs;
          return [
            Number(delta / BigInt(NS_PER_SEC)),
            Number(delta % BigInt(NS_PER_SEC)),
          ];
        }
        return [
          Number(now / BigInt(NS_PER_SEC)),
          Number(now % BigInt(NS_PER_SEC)),
        ];
      };
      hr.bigint = function () {
        return hostHrtime();
      };
    } else {
      hr = function (prev) {
        var ms = Date.now();
        var s = Math.floor(ms / 1000);
        var ns = (ms % 1000) * 1e6;
        if (prev) {
          var ds = s - prev[0];
          var dns = ns - prev[1];
          if (dns < 0) {
            ds -= 1;
            dns += NS_PER_SEC;
          }
          return [ds, dns];
        }
        return [s, ns];
      };
      hr.bigint = function () {
        return BigInt(Math.floor(Date.now() * 1e6));
      };
    }
    return hr;
  }

  hrtimeFn = makeHrtime();

  globalThis.process = {
    argv: ["node", "/bundle.js"],
    argv0: "node",
    execPath: "/usr/bin/node",
    env: {},
    pid: 1,
    ppid: 0,
    platform: "linux",
    arch: "x64",
    version: "v20.0.0",
    versions: { node: "20.0.0" },
    cwd: function () {
      return "/";
    },
    // Node's nextTick runs before Promise microtasks, but jsse has no separate
    // tick queue; a microtask is close enough for library test runners.
    nextTick: function (cb) {
      var extra = Array.prototype.slice.call(arguments, 1);
      Promise.resolve().then(function () {
        cb.apply(undefined, extra);
      });
    },
    stdout: stdout,
    stderr: stderr,
    hrtime: hrtimeFn,
    exit: function (code) {
      code = code ? code | 0 : 0;
      if (hostExit) {
        hostExit(code); // real, uncatchable exit (issue #242)
        return;
      }
      // Fallback: flush buffered output, then let a non-zero code surface as a
      // throw the harness can see.
      stdout._flush();
      stderr._flush();
      if (code) throw new Error("process.exit(" + code + ")");
    },
    on: function () {
      return globalThis.process;
    },
    once: function () {
      return globalThis.process;
    },
    emit: function () {
      return false;
    },
  };

  // ---- console --------------------------------------------------------------
  var groupIndent = "";

  function writeLine(stream, args) {
    var line = format.apply(null, args);
    if (groupIndent) {
      line = groupIndent + line.replace(/\n/g, "\n" + groupIndent);
    }
    stream.write(line + "\n");
  }

  var counts = {};
  var timers = {};

  function timerNow() {
    return hrtimeFn.bigint();
  }

  var jsseConsole = {
    log: function () {
      writeLine(stdout, arguments);
    },
    info: function () {
      writeLine(stdout, arguments);
    },
    debug: function () {
      writeLine(stdout, arguments);
    },
    error: function () {
      writeLine(stderr, arguments);
    },
    warn: function () {
      writeLine(stderr, arguments);
    },
    dir: function (obj, opts) {
      stdout.write((groupIndent || "") + inspect(obj, opts || {}) + "\n");
    },
    trace: function () {
      var msg = format.apply(null, arguments);
      var stack = new Error().stack || "";
      stderr.write("Trace" + (msg ? ": " + msg : "") + "\n" + stack + "\n");
    },
    assert: function (cond) {
      if (cond) return;
      var rest = Array.prototype.slice.call(arguments, 1);
      var msg = rest.length ? ": " + format.apply(null, rest) : "";
      stderr.write("Assertion failed" + msg + "\n");
    },
    group: function () {
      if (arguments.length) writeLine(stdout, arguments);
      groupIndent += "  ";
    },
    groupCollapsed: function () {
      if (arguments.length) writeLine(stdout, arguments);
      groupIndent += "  ";
    },
    groupEnd: function () {
      groupIndent = groupIndent.slice(0, groupIndent.length - 2);
    },
    count: function (label) {
      label = label === undefined ? "default" : String(label);
      counts[label] = (counts[label] || 0) + 1;
      jsseConsole.log(label + ": " + counts[label]);
    },
    countReset: function (label) {
      label = label === undefined ? "default" : String(label);
      counts[label] = 0;
    },
    time: function (label) {
      label = label === undefined ? "default" : String(label);
      timers[label] = timerNow();
    },
    timeEnd: function (label) {
      label = label === undefined ? "default" : String(label);
      if (!(label in timers)) {
        jsseConsole.warn("Warning: No such label '" + label + "'");
        return;
      }
      var ms = Number(timerNow() - timers[label]) / 1e6;
      delete timers[label];
      jsseConsole.log(label + ": " + ms + "ms");
    },
    timeLog: function (label) {
      label = label === undefined ? "default" : String(label);
      if (!(label in timers)) {
        jsseConsole.warn("Warning: No such label '" + label + "'");
        return;
      }
      var ms = Number(timerNow() - timers[label]) / 1e6;
      var rest = Array.prototype.slice.call(arguments, 1);
      var extra = rest.length ? " " + format.apply(null, rest) : "";
      jsseConsole.log(label + ": " + ms + "ms" + extra);
    },
    // Best-effort: Node renders an ASCII table; a readable inspect dump is
    // close enough for the test runners that call it.
    table: function (data) {
      jsseConsole.dir(data, { depth: null });
    },
  };

  // jsse binds `console` as a lexical const (not a plain global-object
  // property), so a `globalThis.console = …` reassignment would be shadowed by
  // bare `console` references in the bundle. Mutate the existing object instead:
  // its native `log` is writable/configurable and the object is extensible, so
  // overriding `log` and adding the rest of the method set takes effect for
  // both `console.x` and bare `console` uses.
  for (var method in jsseConsole) {
    if (Object.prototype.hasOwnProperty.call(jsseConsole, method)) {
      console[method] = jsseConsole[method];
    }
  }
})();
