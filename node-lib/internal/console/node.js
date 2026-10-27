// `console` for `--node`: the shim's console method set formatted by the
// embedded util module (Node's util.inspect/format, node-lib/util.js) instead
// of the shim's best-effort inspect. The builtin loader evaluates this once at
// startup; the util module is loaded on the first call that needs it, and
// calls whose arguments are all plain strings or primitives are formatted
// here. Output goes out through process.stdout/stderr (internal/process/stdio),
// colored like Node's `colorMode: 'auto'` when the stream is a color TTY. The
// shim's global `util` is left as it is.

'use strict';

{
  var hrtime = process.hrtime;
  var util;
  var stdio;
  var groupIndent = "";
  var counts = Object.create(null);
  var timers = Object.create(null);

  function getUtil() {
    return util || (util = require("util"));
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

  function getStdio() {
    return stdio || (stdio = require("internal/process/stdio"));
  }

  function colors(fd) {
    var stream = getStdio().colorStream(fd);
    if (stream === undefined && process.env.FORCE_COLOR === undefined) return false;
    return !!require("internal/util/colors").shouldColorize(stream);
  }

  function format(fd, args) {
    var n = args.length;
    var plain = n > 0;
    var strings = true;
    for (var i = 0; i < n && plain; i++) {
      plain = simple(args[i]) && (i > 0 || n === 1 || typeof args[0] !== "string" ||
        args[0].indexOf("%") === -1);
      if (typeof args[i] !== "string") strings = false;
    }
    if (plain && !strings && colors(fd)) plain = false;
    if (!plain) {
      var list = Array.prototype.slice.call(args);
      return getUtil().formatWithOptions.apply(null, [{ colors: colors(fd) }].concat(list));
    }
    var line = String(args[0]);
    for (var j = 1; j < n; j++) line += " " + String(args[j]);
    return line;
  }

  function write(fd, text) {
    if (groupIndent) {
      text = groupIndent + text.split("\n").join("\n" + groupIndent);
    }
    getStdio().write(fd, text + "\n");
  }

  function label(value) {
    return value === undefined ? "default" : String(value);
  }

  function now() {
    return hrtime.bigint();
  }

  function pad(n) {
    return n < 10 ? "0" + n : String(n);
  }

  function formatTime(ms) {
    var hours = 0;
    var minutes = 0;
    var seconds = 0;
    if (ms >= 1000) {
      if (ms >= 60000) {
        if (ms >= 3600000) {
          hours = Math.floor(ms / 3600000);
          ms = ms % 3600000;
        }
        minutes = Math.floor(ms / 60000);
        ms = ms % 60000;
      }
      seconds = ms / 1000;
    }
    if (hours !== 0 || minutes !== 0) {
      var parts = seconds.toFixed(3).split(".");
      var head = hours !== 0 ? hours + ":" + pad(minutes) : minutes;
      return head + ":" + pad(+parts[0]) + "." + parts[1] +
        " (" + (hours !== 0 ? "h:m" : "") + "m:ss.mmm)";
    }
    if (seconds !== 0) return seconds.toFixed(3) + "s";
    return Number(ms.toFixed(3)) + "ms";
  }

  function elapsed(name, method) {
    if (!(name in timers)) {
      process.emitWarning("No such label '" + name + "' for console." + method + "()");
      return undefined;
    }
    return formatTime(Number(now() - timers[name]) / 1e6);
  }

  var c = {
    log: function () {
      write(1, format(1, arguments));
    },
    error: function () {
      write(2, format(2, arguments));
    },
    dir: function (obj, opts) {
      var options = Object.assign({ customInspect: false, colors: colors(1) }, opts);
      write(1, getUtil().inspect(obj, options));
    },
    trace: function () {
      var msg = format(2, arguments);
      var stack = new Error().stack || "";
      write(2, "Trace" + (msg ? ": " + msg : "") + "\n" + stack);
    },
    assert: function (cond) {
      if (cond) return;
      var rest = Array.prototype.slice.call(arguments, 1);
      if (typeof rest[0] === "string") rest[0] = "Assertion failed: " + rest[0];
      else rest.unshift("Assertion failed");
      write(2, format(2, rest));
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
      name = label(name);
      if (!(name in counts)) {
        process.emitWarning("Count for '" + name + "' does not exist");
        return;
      }
      counts[name] = 0;
    },
    time: function (name) {
      name = label(name);
      if (name in timers) {
        process.emitWarning("Label '" + name + "' already exists for console.time()");
        return;
      }
      timers[name] = now();
    },
    timeEnd: function (name) {
      name = label(name);
      var ms = elapsed(name, "timeEnd");
      if (ms === undefined) return;
      delete timers[name];
      c.log(name + ": " + ms);
    },
    timeLog: function (name) {
      name = label(name);
      var ms = elapsed(name, "timeLog");
      if (ms === undefined) return;
      var rest = Array.prototype.slice.call(arguments, 1);
      c.log(name + ": " + ms + (rest.length ? " " + format(1, rest) : ""));
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
}
