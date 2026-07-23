// Self-test for the Node host-compat readable-output layer (scripts/node-shim.js,
// issue #230). It runs on BOTH jsse (`--node`, with the shim prepended) and Node
// (where the shim is inert) and must produce byte-identical output and exit 0 on
// both. Node is the oracle: the expected literals below are whatever Node emits,
// so a jsse divergence fails the run.
//
// Deterministic surfaces (util.format specifiers, byte-accurate stdout, the
// count/group/assert output shapes) are asserted exactly. util.inspect is
// best-effort (issue #230), so it is only smoke-tested structurally — never
// byte-compared against Node.
//
// This file is not esbuild-bundled: scripts/run-node-shim-selftest.sh simply
// concatenates the shim in front of it. On jsse the shim installs globalThis.util;
// on Node `util` is require-only, so fall back to require().

"use strict";

var util =
  typeof globalThis.util !== "undefined" ? globalThis.util : require("util");

var testNo = 0;

function ok(name) {
  testNo++;
  console.log("ok " + testNo + " - " + name);
}

function eq(actual, expected, name) {
  if (actual !== expected) {
    throw new Error(
      "FAIL " + name + ": expected " + JSON.stringify(expected) +
        " got " + JSON.stringify(actual)
    );
  }
  ok(name);
}

function truthy(cond, name) {
  if (!cond) throw new Error("FAIL " + name + ": condition not met");
  ok(name);
}

// Capture output written during fn() so non-deterministic renderings never
// reach the diffed stream. process.stdout/stderr are the objects the shim's
// console writes through (and Node's console is bound to the same objects), so
// swapping their write method captures both.
function capture(fn) {
  var out = [];
  var err = [];
  var ow = process.stdout.write;
  var ew = process.stderr.write;
  process.stdout.write = function (s) {
    out.push(String(s));
    return true;
  };
  process.stderr.write = function (s) {
    err.push(String(s));
    return true;
  };
  try {
    fn();
  } finally {
    process.stdout.write = ow;
    process.stderr.write = ew;
  }
  return { out: out.join(""), err: err.join("") };
}

console.log("# node-shim self-test");

// ---- util.format: %s ------------------------------------------------------
eq(util.format("%s", "hi"), "hi", "%s string");
eq(util.format("%s", 42), "42", "%s number");
eq(util.format("%s", -0), "-0", "%s negative zero");
eq(util.format("%s", 10n), "10n", "%s bigint");
eq(util.format("%s", true), "true", "%s boolean");
eq(util.format("%s", null), "null", "%s null");
eq(util.format("%s", undefined), "undefined", "%s undefined");
eq(
  util.format("%s", {
    toString: function () {
      return "OBJ";
    },
  }),
  "OBJ",
  "%s object with toString"
);
eq(util.format("%s", [1, 2, 3]), "[ 1, 2, 3 ]", "%s array uses inspect");
eq(util.format("%s", { a: 1 }), "{ a: 1 }", "%s plain object uses inspect");
eq(
  util.format(
    "%s",
    new (class {
      toString() {
        return "CLASS";
      }
    })()
  ),
  "CLASS",
  "%s class-prototype toString uses String"
);
eq(
  util.format("%s", new Date(0)),
  "1970-01-01T00:00:00.000Z",
  "%s Date keeps built-in coercion on inspect path"
);
eq(util.format("%s", /re/g), "/re/g", "%s RegExp uses inspect");
eq(
  util.format("%s", { toString: null, a: 1 }),
  "{ toString: null, a: 1 }",
  "%s non-callable toString uses inspect"
);
(function () {
  var value = {
    [Symbol.toPrimitive]: function (hint) {
      return hint === "string" ? "PRIMITIVE" : "WRONG HINT";
    },
  };
  eq(
    util.format("%s", value),
    "PRIMITIVE",
    "%s own Symbol.toPrimitive uses String with string hint"
  );
})();
(function () {
  function Base() {}
  Base.prototype.toString = function () {
    return "INHERITED";
  };
  function Child() {}
  Object.setPrototypeOf(Child.prototype, Base.prototype);
  eq(
    util.format("%s", new Child()),
    "INHERITED",
    "%s inherited user toString uses String"
  );
})();
(function () {
  function PrimitiveBase() {}
  PrimitiveBase.prototype[Symbol.toPrimitive] = function (hint) {
    return hint === "string" ? "INHERITED PRIMITIVE" : "WRONG HINT";
  };
  function PrimitiveChild() {}
  Object.setPrototypeOf(PrimitiveChild.prototype, PrimitiveBase.prototype);
  eq(
    util.format("%s", new PrimitiveChild()),
    "INHERITED PRIMITIVE",
    "%s inherited user Symbol.toPrimitive uses String"
  );
})();
(function () {
  function Widget() {
    this.a = 1;
  }
  eq(
    util.format("%s", new Widget()),
    "Widget { a: 1 }",
    "%s inherited built-in Object toString uses inspect"
  );
})();
(function () {
  var original = Array.prototype.toString;
  try {
    Array.prototype.toString = function () {
      return "PATCHED ARRAY";
    };
    eq(
      util.format("%s", [1, 2]),
      "[ 1, 2 ]",
      "%s patched built-in prototype still uses inspect"
    );
  } finally {
    Array.prototype.toString = original;
  }
})();
(function () {
  class Array {
    toString() {
      return "USER ARRAY";
    }
  }
  eq(
    util.format("%s", new Array()),
    "Array {}",
    "%s follows Node constructor-name classification"
  );
})();
(function () {
  var value = [1, 2];
  value[Symbol.toPrimitive] = function () {
    return "ARRAY PRIMITIVE";
  };
  eq(
    util.format("%s", value),
    "ARRAY PRIMITIVE",
    "%s own Symbol.toPrimitive overrides built-in array coercion"
  );
})();

// ---- util.format: %d %i %f ------------------------------------------------
eq(util.format("%d", 42), "42", "%d integer");
eq(util.format("%d", 3.5), "3.5", "%d float");
eq(util.format("%d", "7"), "7", "%d numeric string");
eq(util.format("%d", "nope"), "NaN", "%d non-numeric string");
eq(util.format("%d", 10n), "10n", "%d bigint");
eq(util.format("%i", 3.9), "3", "%i truncates");
eq(util.format("%i", "42px"), "42", "%i leading integer");
eq(util.format("%i", 10n), "10n", "%i bigint");
eq(util.format("%f", "3.14xyz"), "3.14", "%f leading float");
eq(util.format("%f", 2), "2", "%f integer");

// ---- util.format: %j %c %% ------------------------------------------------
eq(util.format("%j", { a: 1, b: [2, 3] }), '{"a":1,"b":[2,3]}', "%j json");
eq(util.format("%j", undefined), "undefined", "%j undefined");
(function () {
  var circular = {};
  circular.self = circular;
  eq(util.format("%j", circular), "[Circular]", "%j circular structure -> [Circular]");
  // Node's %j re-throws non-circular serialization errors (BigInt etc.) rather
  // than masking them as [Circular].
  var threw = false;
  try {
    util.format("%j", 1n);
  } catch (e) {
    threw = true;
  }
  truthy(threw, "%j of a BigInt re-throws instead of masking");
})();
eq(util.format("%c", "color:red"), "", "%c ignored (consumes arg)");

// %% only substitutes when at least one argument is present: a lone string is
// returned verbatim (so "%%" stays "%%"), but "%%" with an extra arg becomes "%".
eq(util.format("%%"), "%%", "%% verbatim (single arg)");
eq(util.format("100%% done"), "100%% done", "%% verbatim mid-string (single arg)");
eq(util.format("%%", "x"), "% x", "%% substitutes with extra arg");
eq(util.format("a %% b %s", "c"), "a % b c", "%% substitutes alongside %s");

// ---- util.format: structure -----------------------------------------------
eq(util.format("%s%s", "a", "b"), "ab", "consecutive specifiers");
eq(util.format("%s", "a", "b"), "a b", "extra args appended");
eq(util.format("%s"), "%s", "single-arg string returned verbatim");
eq(util.format("%d and %s"), "%d and %s", "single-arg string with specifiers verbatim");
eq(util.format("plain text"), "plain text", "plain string");
eq(util.format("x=%d y=%s", 1, "two"), "x=1 y=two", "mixed specifiers");
eq(util.format("just %z here", 1), "just %z here 1", "unknown specifier + extra arg");
eq(util.format(), "", "no arguments");
eq(util.format(1, 2, 3), "1 2 3", "non-string first arg");

// ---- util.inspect: best-effort (structural only) --------------------------
(function () {
  var arr = util.inspect([1, 2, 3]);
  truthy(
    arr.indexOf("1") !== -1 && arr.indexOf("2") !== -1 && arr.indexOf("3") !== -1,
    "inspect renders array elements"
  );
  var cyc = {};
  cyc.self = cyc;
  var ci = util.inspect(cyc);
  truthy(typeof ci === "string" && ci.length > 0, "inspect handles cycles without hanging");
  var nested = util.inspect({ a: { b: { c: 1 } } }, { depth: 1 });
  truthy(typeof nested === "string" && nested.length > 0, "inspect respects depth option");
  // Accessors must not be invoked (Node shows [Getter]); a throwing getter must
  // not make inspect throw. This IS deterministic across engines, so assert it.
  var throwingGetter = Object.defineProperty({}, "x", {
    get: function () {
      throw new Error("boom");
    },
    enumerable: true,
    configurable: true,
  });
  eq(util.inspect(throwingGetter), "{ x: [Getter] }", "inspect does not invoke getters");
  var arrGetter = [1];
  Object.defineProperty(arrGetter, "1", {
    get: function () {
      throw new Error("boom");
    },
    enumerable: true,
    configurable: true,
  });
  eq(util.inspect(arrGetter), "[ 1, [Getter] ]", "inspect does not invoke array getters");
  // The constructor-name prefix must not invoke an accessor `constructor` or a
  // Proxy get-trap (Node derives it from prototype metadata).
  var ctorGetter = { a: 1 };
  Object.defineProperty(ctorGetter, "constructor", {
    get: function () {
      throw new Error("boom");
    },
    enumerable: false,
    configurable: true,
  });
  eq(util.inspect(ctorGetter), "{ a: 1 }", "inspect does not invoke a constructor getter");
  var ctorProxy = new Proxy(
    { a: 1 },
    {
      get: function (t, k) {
        if (k === "constructor") throw new Error("trap");
        return t[k];
      },
    }
  );
  var inspectedProxy = util.inspect(ctorProxy);
  truthy(
    inspectedProxy === "{ a: 1 }" || inspectedProxy === "Proxy({ a: 1 })",
    "inspect does not trip a Proxy constructor trap"
  );
  // A normal named class still gets its "ClassName " prefix.
  function Widget() {
    this.a = 1;
  }
  eq(util.inspect(new Widget()), "Widget { a: 1 }", "inspect keeps the class-name prefix");
})();

// ---- process fields -------------------------------------------------------
truthy(typeof process.platform === "string" && process.platform.length > 0, "process.platform");
truthy(typeof process.arch === "string" && process.arch.length > 0, "process.arch");
truthy(process.versions && typeof process.versions.node === "string", "process.versions.node");
truthy(typeof process.version === "string" && process.version[0] === "v", "process.version");
truthy(Array.isArray(process.argv) && process.argv.length >= 2, "process.argv");
truthy(typeof process.env === "object" && process.env !== null, "process.env");
truthy(typeof process.cwd === "function" && typeof process.cwd() === "string", "process.cwd()");
truthy(typeof process.pid === "number", "process.pid");

// ---- process.hrtime -------------------------------------------------------
(function () {
  var h = process.hrtime();
  truthy(
    Array.isArray(h) && h.length === 2 && typeof h[0] === "number" && typeof h[1] === "number",
    "process.hrtime() returns [seconds, nanoseconds]"
  );
  truthy(typeof process.hrtime.bigint() === "bigint", "process.hrtime.bigint() returns a BigInt");
})();

// ---- process.nextTick -----------------------------------------------------
var tickRan = false;
process.nextTick(function () {
  tickRan = true;
});

// ---- console: count / group / assert / time (captured) --------------------
(function () {
  var c = capture(function () {
    console.count("widget");
    console.count("widget");
    console.countReset("widget");
    console.count("widget");
  });
  eq(c.out, "widget: 1\nwidget: 2\nwidget: 1\n", "console.count / countReset");

  var g = capture(function () {
    console.group("Section");
    console.log("nested");
    console.groupEnd();
    console.log("flush");
  });
  eq(g.out, "Section\n  nested\nflush\n", "console.group indentation");

  var a = capture(function () {
    console.assert(false, "boom");
    console.assert(true, "quiet");
  });
  eq(a.err, "Assertion failed: boom\n", "console.assert on failure");

  var t = capture(function () {
    console.time("phase");
    console.timeEnd("phase");
  });
  truthy(/^phase: [\d.]+(ms|s|m|h|min)\n$/.test(t.out), "console.timeEnd format");

  var d = capture(function () {
    console.dir({ hello: 1 });
  });
  truthy(d.out.indexOf("hello") !== -1 && d.out.indexOf("1") !== -1, "console.dir renders object");

  var e = capture(function () {
    console.error("err %s", "msg");
    console.warn("warn %d", 5);
  });
  eq(e.err, "err msg\nwarn 5\n", "console.error / warn format to stderr");
})();

// ---- console.log formatting reaches real stdout (diffed) ------------------
console.log("formatted: %s=%d", "count", 3);
console.log("multi", "args", 7);

// ---- byte-accurate process.stdout.write (no implicit newline) -------------
process.stdout.write("A");
process.stdout.write("B");
process.stdout.write("C\n");

// nextTick must have run by end of the synchronous phase's microtask drain.
Promise.resolve().then(function () {
  truthy(tickRan, "process.nextTick scheduled a callback");
  console.log("1.." + testNo);
  console.log("# all passed");
});
