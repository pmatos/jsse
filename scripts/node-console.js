// `console` for `--node`: the shim's console method set formatted by the
// embedded util module (Node's util.inspect/format, node-lib/util.js) instead
// of the shim's best-effort inspect. The module is loaded on the first call
// that needs it; calls whose arguments are all plain strings or primitives are
// formatted here. The shim's global `util` is left as it is until the
// CommonJS loader (#1219) lands.

(function () {
  "use strict";

  if (typeof __node_builtin !== "function") return;

  var loadBuiltin = __node_builtin;
  var hrtime = globalThis.process.hrtime;
  var util;
  var groupIndent = "";
  var counts = Object.create(null);
  var timers = Object.create(null);

  function getUtil() {
    return util || (util = loadBuiltin("util"));
  }

  function simple(value) {
    switch (typeof value) {
      case "string":
      case "boolean":
      case "undefined":
        return true;
      case "number":
        return value !== 0 || 1 / value > 0;
      default:
        return value === null;
    }
  }

  function format(args) {
    var n = args.length;
    var plain = n > 0;
    for (var i = 0; i < n && plain; i++) {
      plain = simple(args[i]) && (i > 0 || n === 1 || typeof args[0] !== "string" ||
        args[0].indexOf("%") === -1);
    }
    if (!plain) return getUtil().format.apply(null, args);
    var line = String(args[0]);
    for (var j = 1; j < n; j++) line += " " + String(args[j]);
    return line;
  }

  function write(stream, text) {
    if (groupIndent) {
      text = groupIndent + text.split("\n").join("\n" + groupIndent);
    }
    stream.write(text + "\n");
  }

  function label(value) {
    return value === undefined ? "default" : String(value);
  }

  function now() {
    return hrtime.bigint();
  }

  function elapsed(name) {
    if (!(name in timers)) {
      c.warn("Warning: No such label '" + name + "'");
      return undefined;
    }
    return Number(now() - timers[name]) / 1e6;
  }

  var c = {
    log: function () {
      write(process.stdout, format(arguments));
    },
    error: function () {
      write(process.stderr, format(arguments));
    },
    dir: function (obj, opts) {
      var options = Object.assign({ customInspect: false }, opts);
      write(process.stdout, getUtil().inspect(obj, options));
    },
    trace: function () {
      var msg = format(arguments);
      var stack = new Error().stack || "";
      process.stderr.write("Trace" + (msg ? ": " + msg : "") + "\n" + stack + "\n");
    },
    assert: function (cond) {
      if (cond) return;
      var rest = Array.prototype.slice.call(arguments, 1);
      var msg = rest.length ? ": " + format(rest) : "";
      process.stderr.write("Assertion failed" + msg + "\n");
    },
    group: function () {
      if (arguments.length) c.log.apply(c, arguments);
      groupIndent += "  ";
    },
    groupEnd: function () {
      groupIndent = groupIndent.slice(0, groupIndent.length - 2);
    },
    count: function (name) {
      name = label(name);
      counts[name] = (counts[name] || 0) + 1;
      c.log(name + ": " + counts[name]);
    },
    countReset: function (name) {
      counts[label(name)] = 0;
    },
    time: function (name) {
      timers[label(name)] = now();
    },
    timeEnd: function (name) {
      name = label(name);
      var ms = elapsed(name);
      if (ms === undefined) return;
      delete timers[name];
      c.log(name + ": " + ms + "ms");
    },
    timeLog: function (name) {
      name = label(name);
      var ms = elapsed(name);
      if (ms === undefined) return;
      var rest = Array.prototype.slice.call(arguments, 1);
      c.log(name + ": " + ms + "ms" + (rest.length ? " " + format(rest) : ""));
    },
    // Node renders an ASCII table; an inspect dump is close enough here.
    table: function (data) {
      c.dir(data, { depth: null });
    },
  };
  c.info = c.debug = c.log;
  c.warn = c.error;
  c.groupCollapsed = c.group;

  for (var method in c) {
    console[method] = c[method];
  }
})();
