// Stack-shape cases for the V8-compatible Error API (issue #1228), run
// verbatim on Node and on `jsse --node` by scripts/run-node-v8-error-oracle.sh,
// which normalises paths and drops frames outside this file. Everything runs
// inside functions: the top-level frame differs between the two hosts.
function show(label, value) {
  console.log("## " + label);
  console.log(value);
}

function inner() {
  return new Error("boom");
}
function middle() {
  return inner();
}
function outer() {
  return middle();
}

const literal = {
  method() {
    return new Error("method");
  },
  get accessor() {
    return new Error("getter");
  },
};

class Widget {
  constructor() {
    this.error = new Error("ctor");
  }
  static make() {
    return new Widget();
  }
  run() {
    return new Error("instance");
  }
}

class CustomError extends Error {
  constructor(message) {
    super(message);
    this.name = "CustomError";
  }
}

class Derived extends CustomError {}

function throwsCustom() {
  throw new Derived("deep");
}

function caseFrames() {
  show("plain chain", outer().stack);
  show("object method", literal.method().stack);
  show("getter", literal.accessor.stack.split("\n").slice(0, 2).join("\n"));
  show("constructor", new Widget().error.stack);
  show("static method", Widget.make().error.stack);
  show("instance method", new Widget().run().stack);
  show("callback", [1].map(function cb() { return new Error("cb"); })[0].stack);
  show("call without new", (function viaCall() { return Error("nonew"); })().stack);
  show("native error types", [new TypeError("t"), new RangeError("r")].map((e) => e.stack.split("\n")[0]).join("|"));
  try {
    throwsCustom();
  } catch (e) {
    show("subclass chain", e.stack);
    show("subclass name", e instanceof CustomError && e.name);
  }
}

function caseCapture() {
  function target(obj) {
    Error.captureStackTrace(obj);
    return obj;
  }
  function viaHelper() {
    const holder = {};
    function helper() {
      Error.captureStackTrace(holder, helper);
    }
    helper();
    return holder;
  }
  show("capture plain object", target({}).stack);
  show("capture named object", target({ name: "N", message: "M" }).stack);
  show("capture with constructorOpt", viaHelper().stack);
  show("capture descriptor", JSON.stringify(Object.keys(Object.getOwnPropertyDescriptor(target({}), "stack")).sort()));
  const missing = {};
  Error.captureStackTrace(missing, function notOnStack() {});
  show("constructorOpt not on stack", missing.stack);
  let threw;
  try {
    Error.captureStackTrace(1);
  } catch (e) {
    threw = e instanceof TypeError;
  }
  show("capture on a primitive", threw);
}

function caseLimit() {
  const saved = Error.stackTraceLimit;
  const lines = (e) =>
    String(e.stack)
      .split("\n")
      .filter((l) => /cases\.js/.test(l) && !/Object\.<anonymous>/.test(l)).length;
  for (const limit of [0, 1, 2, 100]) {
    Error.stackTraceLimit = limit;
    show("limit " + limit, lines(outer()));
  }
  Error.stackTraceLimit = "ten";
  show("limit non-number", outer().stack);
  Error.stackTraceLimit = saved;
  show("limit default", saved);
  show("limit descriptor", JSON.stringify(Object.getOwnPropertyDescriptor(Error, "stackTraceLimit")));
}

function caseHeader() {
  const late = new Error("late");
  late.name = "Renamed";
  late.message = "changed";
  show("header read after rename", late.stack.split("\n")[0]);
  const early = new Error("early");
  early.stack;
  early.name = "Ignored";
  show("header fixed once read", early.stack.split("\n")[0]);
  show("empty message", new Error().stack.split("\n")[0]);
  show("multi-line message", new Error("a\nb").stack.split("\n").slice(0, 2).join("|"));
  const custom = new Error("assigned");
  custom.stack = "custom stack";
  show("assigned stack", custom.stack);
  show("own stack keys", JSON.stringify(Object.getOwnPropertyNames(new Error("k")).sort()));
  show("stack enumerable", Object.prototype.propertyIsEnumerable.call(new Error("e"), "stack"));
}

function caseCallSites() {
  const saved = Error.prepareStackTrace;
  Error.prepareStackTrace = function (error, sites) {
    return sites.slice(0, 3).map((site) => [
      site.getFunctionName(),
      site.getTypeName(),
      typeof site.getFileName(),
      site.getLineNumber(),
      site.getColumnNumber(),
      site.isConstructor(),
      site.isNative(),
      site.isEval(),
    ].join(",")).join("\n");
  };
  show("prepareStackTrace sites", Widget.make().error.stack);
  Error.prepareStackTrace = function (error, sites) {
    return { sites: Array.isArray(sites), same: error instanceof Error, count: sites.length > 0 };
  };
  show("prepareStackTrace object result", JSON.stringify(outer().stack));
  let calls = 0;
  Error.prepareStackTrace = function () {
    calls++;
    return "prepared";
  };
  const lazy = new Error("lazy");
  const before = calls;
  const first = lazy.stack;
  lazy.stack;
  show("prepareStackTrace runs once, on first read", [before, calls, first].join(","));
  Error.prepareStackTrace = function (error, sites) {
    return error.name + "!" + sites.map(String).join("|").length;
  };
  show("site toString joined", typeof outer().stack);
  Error.prepareStackTrace = saved;
  show("restored", outer().stack.split("\n")[0]);
  show("site toString", (function () {
    Error.prepareStackTrace = (e, sites) => sites.slice(0, 2).map(String).join("\n");
    const text = Widget.make().error.stack;
    Error.prepareStackTrace = saved;
    return text;
  })());
}

function caseCauseDisplay() {
  const cause = new TypeError("inner");
  cause.stack = "TypeError: inner\n    at inner.js:1:1";
  const err = new Error("outer", { cause });
  err.stack = "Error: outer\n    at outer.js:2:2";
  show("cause", err);
  const plain = new Error("plain");
  plain.stack = "Error: plain\n    at plain.js:3:3";
  plain.code = "E_PLAIN";
  show("extra property", plain);
  const scalar = new Error("scalar", { cause: "text" });
  scalar.stack = "Error: scalar\n    at scalar.js:4:4";
  show("scalar cause", scalar);
  const bare = new Error("bare");
  bare.stack = "Error: bare";
  show("no frames", bare);
  const noStack = new Error("gone", { cause: 1 });
  noStack.stack = "Error: gone";
  show("no frames with cause", noStack);
}

async function beforeAwait() {
  return new Error("before await").stack;
}
async function afterAwait() {
  await null;
  return new Error("after await").stack;
}

function caseAsync() {
  beforeAwait().then((stack) => show("async before await", stack));
  afterAwait().then((stack) => show("async after await", stack));
}

caseFrames();
caseCapture();
caseLimit();
caseHeader();
caseCallSites();
caseCauseDisplay();
caseAsync();
