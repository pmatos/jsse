// Frame names and error key order for the V8-compatible Error API (issue
// #1379), run verbatim on Node and on `jsse --node` by
// scripts/run-node-v8-error-oracle.sh. Inferred names of member assignments,
// receiver type names of arrows (including async ones) and the own-key order
// of constructed errors. Everything runs inside functions: the top-level frame
// differs between the two hosts.
function show(label, value) {
  console.log("## " + label);
  console.log(value);
}

function Context() {}

function keys(error) {
  return JSON.stringify(Object.getOwnPropertyNames(error));
}

function caseKeys() {
  show("keys: message", keys(new Error("m")));
  show("keys: no message", keys(new Error()));
  show("keys: cause", keys(new TypeError("m", { cause: 1 })));
  show("keys: aggregate", keys(new AggregateError([], "m")));
  show("keys: subclass", keys(new (class Mine extends RangeError {})("m")));
  try {
    null.x;
  } catch (e) {
    show("keys: engine error", keys(e));
  }
  const plain = { a: 1 };
  Error.captureStackTrace(plain);
  show("keys: captureStackTrace appends", keys(plain));
}

function caseInferred() {
  const exports = {};
  exports.validateLegacyPlugin = function () {
    return new Error("exports");
  };
  show("exports.member", exports.validateLegacyPlugin().stack);
  const aliased = exports.validateLegacyPlugin;
  show("alias keeps the inferred name", aliased().stack);

  const a = { b: { deep: {} } };
  a.b.deep.fn = function () {
    return new Error("deep");
  };
  show("nested member", a.b.deep.fn().stack);

  a["str"] = function () {
    return new Error("string key");
  };
  show("string-literal key", a.str().stack);

  a[0] = function () {
    return new Error("index key");
  };
  show("index key", a[0]().stack);

  a.arrow = () => new Error("arrow");
  show("arrow member", a.arrow().stack);

  a.nullish ??= function () {
    return new Error("nullish");
  };
  show("logical assignment", a.nullish().stack);

  a.choice = true
    ? function () {
        return new Error("conditional");
      }
    : null;
  show("conditional branch", a.choice().stack);

  a.paren = function () {
    return (() => new Error("body paren"))();
  };
  show("parenthesis inside the body", a.paren().stack);
  const wrapper = {};
  wrapper.fn = (function () {
    return new Error("parenthesized function");
  });
  show("parenthesized function", wrapper.fn().stack);
  wrapper.arrow = (() => new Error("parenthesized arrow"));
  show("parenthesized arrow", wrapper.arrow().stack);
  wrapper.cls = (class {
    constructor() {
      this.error = new Error("parenthesized class");
    }
  });
  show("parenthesized class", new wrapper.cls().error.stack);
  a.first = a.second = function () {
    return new Error("chain");
  };
  show("assignment chain", a.second().stack);

  function Widget() {}
  Widget.prototype.render = function () {
    return new Error("prototype");
  };
  show("prototype member", new Widget().render().stack);
  Widget.build = function () {
    return new Error("static");
  };
  show("static member", Widget.build().stack);

  a.klass = class {
    constructor() {
      this.error = new Error("class");
    }
  };
  show("class", new a.klass().error.stack);

  const self = {};
  (function () {
    this.viaThis = function () {
      return new Error("this");
    };
    show("this member", this.viaThis().stack);
  }).call(self);

  const named = {};
  named.field = function field() {
    return new Error("named");
  };
  show("a real name wins", named.field().stack);

  const names = {};
  names.fn = function () {
    return new Error("names");
  };
  const info = [];
  Error.prepareStackTrace = (error, sites) => {
    info.push([sites[0].getFunctionName(), sites[0].getMethodName(), sites[0].getTypeName()]);
    return "";
  };
  names.fn().stack;
  Error.prepareStackTrace = undefined;
  show("call site names", JSON.stringify(info));
}

function caseArrowReceivers() {
  const context = new Context();
  const arrow = () => new Error("arrow");
  show("arrow.call(object)", arrow.call(context).stack);
  show("arrow.call(string)", arrow.call("text").stack);
  show("arrow.call(number)", arrow.call(5).stack);
  show("arrow.call(null prototype)", arrow.call(Object.create(null)).stack);
  show("arrow.call(undefined)", arrow.call(undefined).stack);
  const run = (fn) => fn.call(context);
  show("callback", run(() => new Error("callback")).stack);
}

const asyncSynchronous = () => {
  let error;
  const sync = async () => {
    error = new Error("async sync");
  };
  sync.call(new Context());
  return error;
};

function caseAsyncArrows() {
  show("async arrow body", asyncSynchronous().stack);
}

caseKeys();
caseInferred();
caseArrowReceivers();
caseAsyncArrows();
