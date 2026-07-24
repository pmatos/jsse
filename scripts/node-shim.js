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
    s = stringConstructor(s);
    return (
      "'" +
      stringReplace(
        stringReplace(stringReplace(s, /\\/g, "\\\\"), /'/g, "\\'"),
        /\n/g,
        "\\n"
      ) +
      "'"
    );
  }

  function isIdentifierKey(k) {
    return regexpTest(/^[A-Za-z_$][A-Za-z0-9_$]*$/, k);
  }

  // Capture uncurried intrinsics before bundled library code runs. Node's
  // formatter reads built-in internal slots rather than user-overridable
  // prototype methods.
  var functionCall = Function.prototype.call;
  var arrayConstructor = Array;
  var bigintConstructor = BigInt;
  var booleanConstructor = Boolean;
  var dateConstructor = Date;
  var errorConstructor = Error;
  var numberConstructor = Number;
  var objectConstructor = Object;
  var regexpConstructor = RegExp;
  var stringConstructor = String;
  var symbolConstructor = Symbol;
  var functionHasInstance = functionCall.bind(
    Function.prototype[symbolConstructor.hasInstance]
  );
  var arrayIndexOf = functionCall.bind(arrayConstructor.prototype.indexOf);
  var arrayIsArray = arrayConstructor.isArray;
  var arrayJoin = functionCall.bind(arrayConstructor.prototype.join);
  var objectGetOwnPropertyDescriptor =
    objectConstructor.getOwnPropertyDescriptor;
  var objectGetPrototypeOf = objectConstructor.getPrototypeOf;
  var objectIs = objectConstructor.is;
  var objectKeys = objectConstructor.keys;
  var objectToString = functionCall.bind(objectConstructor.prototype.toString);
  var numberIsNaN = numberConstructor.isNaN;
  var dateGetTime = functionCall.bind(dateConstructor.prototype.getTime);
  var dateToISOString = functionCall.bind(
    dateConstructor.prototype.toISOString
  );
  var errorToString = functionCall.bind(errorConstructor.prototype.toString);
  var regexpGetSource = functionCall.bind(
    objectGetOwnPropertyDescriptor(regexpConstructor.prototype, "source").get
  );
  var regexpToString = functionCall.bind(
    regexpConstructor.prototype.toString
  );
  var regexpTest = functionCall.bind(regexpConstructor.prototype.test);
  var numberValueOf = functionCall.bind(numberConstructor.prototype.valueOf);
  var stringValueOf = functionCall.bind(stringConstructor.prototype.valueOf);
  var booleanValueOf = functionCall.bind(
    booleanConstructor.prototype.valueOf
  );
  var bigintValueOf = functionCall.bind(bigintConstructor.prototype.valueOf);
  var symbolToString = functionCall.bind(
    symbolConstructor.prototype.toString
  );
  var stringReplace = functionCall.bind(stringConstructor.prototype.replace);

  function tryApplyIntrinsic(intrinsic, value) {
    try {
      return { value: intrinsic(value) };
    } catch (e) {
      // `instanceof` also accepts objects that merely inherit a built-in
      // prototype. Only a genuine instance has the corresponding internal slot.
      return null;
    }
  }

  function tryInstanceOf(value, constructor) {
    try {
      return value instanceof constructor;
    } catch (e) {
      return false;
    }
  }

  function inspect(value, opts) {
    opts = opts || {};
    var maxDepth = typeof opts.depth === "number" ? opts.depth : 2;
    var seen = [];

    function render(v, depth) {
      var t = typeof v;
      if (v === null) return "null";
      if (t === "undefined") return "undefined";
      if (t === "string") return quoteString(v);
      if (t === "number") return objectIs(v, -0) ? "-0" : stringConstructor(v);
      if (t === "bigint") return stringConstructor(v) + "n";
      if (t === "boolean") return stringConstructor(v);
      if (t === "symbol") return v.toString();
      if (t === "function") {
        return "[Function" + (v.name ? ": " + v.name : " (anonymous)") + "]";
      }

      // Objects.
      if (arrayIndexOf(seen, v) !== -1) return "[Circular *1]";
      if (functionHasInstance(errorConstructor, v)) {
        var stack;
        try {
          stack = v.stack;
        } catch (e) {
          // Node ignores a throwing stack getter and renders the intrinsic
          // Error string as a stackless error.
        }
        if (stack) return stringConstructor(stack);
        try {
          return "[" + errorToString(v) + "]";
        } catch (e) {
          return objectToString(v);
        }
      }
      var boxed;
      if (functionHasInstance(regexpConstructor, v)) {
        boxed = tryApplyIntrinsic(regexpGetSource, v);
        if (boxed) return regexpToString(v);
      }
      if (functionHasInstance(dateConstructor, v)) {
        boxed = tryApplyIntrinsic(dateGetTime, v);
        if (boxed) {
          return numberIsNaN(boxed.value) ? "Invalid Date" : dateToISOString(v);
        }
      }
      if (functionHasInstance(numberConstructor, v)) {
        boxed = tryApplyIntrinsic(numberValueOf, v);
        if (boxed) return "[Number: " + render(boxed.value, depth) + "]";
      }
      if (functionHasInstance(stringConstructor, v)) {
        boxed = tryApplyIntrinsic(stringValueOf, v);
        if (boxed) return "[String: " + render(boxed.value, depth) + "]";
      }
      if (functionHasInstance(booleanConstructor, v)) {
        boxed = tryApplyIntrinsic(booleanValueOf, v);
        if (boxed) return "[Boolean: " + render(boxed.value, depth) + "]";
      }
      if (functionHasInstance(bigintConstructor, v)) {
        boxed = tryApplyIntrinsic(bigintValueOf, v);
        if (boxed) {
          // Unlike the older wrappers above, Node's boxed BigInt/Symbol
          // rendering intentionally observes a constructor's current
          // @@hasInstance result. A false or throwing hook selects its generic
          // object shape, but must not intercept the internal-slot probe.
          return tryInstanceOf(v, bigintConstructor)
            ? "[BigInt: " + render(boxed.value, depth) + "]"
            : "Object [BigInt] {}";
        }
      }
      if (functionHasInstance(symbolConstructor, v)) {
        boxed = tryApplyIntrinsic(symbolToString, v);
        if (boxed) {
          return tryInstanceOf(v, symbolConstructor)
            ? "[Symbol: " + boxed.value + "]"
            : "Object [Symbol] {}";
        }
      }

      if (depth < 0) return arrayIsArray(v) ? "[Array]" : "[Object]";

      seen.push(v);
      var out;
      try {
        if (arrayIsArray(v)) {
          var items = [];
          for (var i = 0; i < v.length; i++) items.push(renderMember(v, i, depth));
          out = items.length ? "[ " + arrayJoin(items, ", ") + " ]" : "[]";
        } else {
          var keys = objectKeys(v);
          var parts = [];
          for (var j = 0; j < keys.length; j++) {
            var k = keys[j];
            var label = isIdentifierKey(k) ? k : quoteString(k);
            parts.push(label + ": " + renderMember(v, k, depth));
          }
          var ctorName = constructorName(v);
          out = parts.length
            ? ctorName + "{ " + arrayJoin(parts, ", ") + " }"
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
      var desc = objectGetOwnPropertyDescriptor(container, key);
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
        var own = objectGetOwnPropertyDescriptor(v, "constructor");
        if (own) {
          if (!own.get && !own.set) ctor = own.value;
        } else {
          var proto = objectGetPrototypeOf(v);
          var pd = proto
            ? objectGetOwnPropertyDescriptor(proto, "constructor")
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
  // Node creates this set from globalThis while internal/util/inspect is
  // bootstrapping. By the time user code runs, Node and jsse have both added
  // more globals, but those late names are deliberately absent from Node's
  // classifier. Keep the Node 26.5.0 bootstrap membership explicit so jsse-only
  // globals (for example ShadowRealm) cannot change %s dispatch.
  var builtInObjectNames = (function () {
    var names = Object.create(null);
    var nodeBootstrapNames = [
      "Object",
      "Function",
      "Array",
      "Number",
      "Infinity",
      "NaN",
      "Boolean",
      "String",
      "Symbol",
      "Date",
      "Promise",
      "RegExp",
      "Error",
      "AggregateError",
      "EvalError",
      "RangeError",
      "ReferenceError",
      "SyntaxError",
      "TypeError",
      "URIError",
      "JSON",
      "Math",
      "Intl",
      "ArrayBuffer",
      "Atomics",
      "Uint8Array",
      "Int8Array",
      "Uint16Array",
      "Int16Array",
      "Uint32Array",
      "Int32Array",
      "BigUint64Array",
      "BigInt64Array",
      "Uint8ClampedArray",
      "Float32Array",
      "Float64Array",
      "DataView",
      "Map",
      "BigInt",
      "Set",
      "Iterator",
      "WeakMap",
      "WeakSet",
      "Proxy",
      "Reflect",
      "FinalizationRegistry",
      "WeakRef",
    ];
    for (var i = 0; i < nodeBootstrapNames.length; i++) {
      names[nodeBootstrapNames[i]] = true;
    }
    return names;
  })();
  var objectHasOwnProperty = objectConstructor.prototype.hasOwnProperty;
  var symbolToPrimitive = symbolConstructor.toPrimitive;

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
      pointer = objectGetPrototypeOf(pointer);
    } while (
      pointer !== null &&
      !hasOwnToString(pointer, "toString") &&
      !hasOwnToPrimitive(pointer, symbolToPrimitive)
    );

    // A callable hook visible through a Proxy get trap may not have an owner in
    // the reported prototype chain. Node can unwrap proxies internally; the
    // pure-JS shim cannot, so treat that hook as user-defined.
    if (pointer === null) return false;

    var descriptor = objectGetOwnPropertyDescriptor(pointer, "constructor");
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
