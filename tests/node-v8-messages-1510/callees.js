// V8 text of callees that are not a name chain, and of destructuring and
// iteration messages (#1512), run verbatim on Node and on `jsse --node` by
// scripts/run-node-v8-messages-1510-oracle.sh. Every case prints the error's
// name and message, and its top stack frame unless the case is listed in
// `messageOnly` (the position differs; see known-failures.md).
const o = {};
const n = null;
const u = undefined;
const num = 5;
function f() {}

const cases = {
  voidCall() {
    (void 0)();
  },
  commaCall() {
    (0, o.f)();
  },
  commaCallThree() {
    (0, f, o.f)();
  },
  logicalCall() {
    (o.f || o.g)();
  },
  typeofCall() {
    (typeof num)();
  },
  parenthesizedLiteral() {
    (1)();
  },
  stringCall() {
    "abc"();
  },
  numberCall() {
    5();
  },
  nullCall() {
    null();
  },
  objectLiteralCall() {
    ({})();
  },
  functionResultCall() {
    (function () {})()();
  },
  arrowResultCall() {
    (() => 1)()();
  },
  callResultCall() {
    f()();
  },
  newComma() {
    new (0, o.F)();
  },
  newVoid() {
    new (void 0)();
  },
  newCall() {
    new (f())();
  },
  newArrow() {
    new (() => {})();
  },
  newGenerator() {
    new (function* () {})();
  },
  destructureNullLiteral() {
    var { a } = null;
  },
  destructureVariable() {
    var { a } = n;
  },
  destructureUndefined() {
    var { a } = u;
  },
  destructureMember() {
    var { a } = o.p;
  },
  destructureCall() {
    var { a } = f();
  },
  destructureLet() {
    let { a } = n;
  },
  destructureConst() {
    const { a } = n;
  },
  destructureNoProperty() {
    var {} = n;
  },
  destructureRest() {
    var { ...r } = n;
  },
  destructureComputed() {
    var { [num]: b } = n;
  },
  destructureStringKey() {
    var { s: b, t: c } = n;
  },
  destructureAssign() {
    let a;
    ({ a } = n);
  },
  destructureAssignMember() {
    let a;
    ({ a } = o.p);
  },
  destructureParameter() {
    (function ({ a }) {})();
  },
  destructureParameterNull() {
    (function ({ a }) {})(null);
  },
  destructureArrow() {
    (({ a }) => a)();
  },
  destructureNested() {
    var {
      a: { b },
    } = { a: null };
  },
  arrayNullLiteral() {
    var [a] = null;
  },
  arrayVariable() {
    var [a] = n;
  },
  arrayUndefined() {
    var [a] = u;
  },
  arrayMember() {
    var [a] = o.p;
  },
  arrayCall() {
    var [a] = f();
  },
  arrayEmpty() {
    var [] = n;
  },
  arrayAssign() {
    let a;
    [a] = n;
  },
  arrayParameter() {
    (function ([a]) {})();
  },
  forOfLiteral() {
    for (const x of 5);
  },
  forOfVariable() {
    for (const x of num);
  },
  forOfNull() {
    for (const x of n);
  },
  forOfObject() {
    for (const x of o);
  },
  forOfMember() {
    for (const x of o.p);
  },
  forOfCall() {
    for (const x of f());
  },
  forOfObjectLiteral() {
    for (const x of {});
  },
  forOfVar() {
    for (var x of num);
  },
  forOfAssign() {
    let x;
    for (x of num);
  },
  spreadLiteral() {
    return [...5];
  },
  spreadVariable() {
    return [...num];
  },
  spreadObject() {
    return [...o];
  },
  spreadNull() {
    return [...n];
  },
  spreadMember() {
    return [...o.p];
  },
  spreadCall() {
    return [...f()];
  },
};

// Message only: the position V8 reports for these is not the engine's.
const messageOnly = new Set([
  "destructureNullLiteral",
  "destructureVariable",
  "destructureUndefined",
  "destructureMember",
  "destructureCall",
  "destructureLet",
  "destructureConst",
  "destructureNoProperty",
  "destructureRest",
  "destructureComputed",
  "destructureStringKey",
  "destructureAssign",
  "destructureAssignMember",
  "destructureParameter",
  "destructureParameterNull",
  "destructureArrow",
  "destructureNested",
  "arrayNullLiteral",
  "arrayVariable",
  "arrayUndefined",
  "arrayMember",
  "arrayEmpty",
  "arrayParameter",
  "forOfLiteral",
  "forOfVariable",
  "forOfNull",
  "forOfObject",
  "forOfMember",
  "forOfObjectLiteral",
  "forOfVar",
  "forOfAssign",
  "spreadLiteral",
  "spreadVariable",
  "spreadObject",
  "spreadNull",
  "spreadMember",
]);

for (const name of Object.keys(cases)) {
  try {
    cases[name]();
    console.log("## " + name + "\nno error");
  } catch (e) {
    const frame = messageOnly.has(name) ? "" : String(e.stack).split("\n")[1];
    const head = "## " + name + "\n" + e.name + ": " + e.message;
    console.log(frame ? head + "\n" + frame.trim() : head);
  }
}
