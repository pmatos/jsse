/*---
description: >
  In an object literal that contains a `yield` or `await`, a shorthand
  `__proto__` property defines an own data property instead of setting the
  prototype, while the `__proto__: value` form still sets it. Checked in
  generators the engine hosts as a bytecode chunk, in generators it runs as a
  state machine, async functions and async generators.
esid: sec-__proto__-property-names-in-object-initializers
info: |
  PropertyDefinition : IdentifierReference

  1. Let propName be the StringValue of IdentifierReference.
  2. Let exprValue be ? Evaluation of IdentifierReference.
  3. Let propValue be ? GetValue(exprValue).
  4. Assert: enumerable is true.
  5. Assert: object is an ordinary, extensible object with no non-configurable properties.
  6. Perform ! CreateDataPropertyOrThrow(object, propName, propValue).

  PropertyDefinition : PropertyName : AssignmentExpression

  7. If isProtoSetter is true, then ... Perform ! object.[[SetPrototypeOf]](propValue).
flags: [async]
features: [generators, async-functions, async-iteration]
---*/

var GeneratorFunction = Object.getPrototypeOf(function*() {}).constructor;
var AsyncFunction = Object.getPrototypeOf(async function() {}).constructor;
var AsyncGeneratorFunction = Object.getPrototypeOf(async function*() {}).constructor;

// `@` stands for `yield ` or `await `.
var prelude = "var pv = { x: 1 }; var __proto__ = pv;" +
  "function describe(o) { return [Object.getPrototypeOf(o) === pv, Object.hasOwn(o, '__proto__'), o.y].join(); }";

var cases = [
  ["shorthandBeforeSuspension", "return describe({ __proto__, y: @2 });", "false,true,2"],
  ["shorthandAfterSuspension", "return describe({ y: @2, __proto__ });", "false,true,2"],
  ["shorthandBeforeSuspendingComputedKey", "return describe({ __proto__, [@'y']: 2 });", "false,true,2"],
  ["protoSetterStillSetsPrototype", "return describe({ __proto__: pv, y: @2 });", "true,false,2"],
  ["protoSetterWithSuspendingValue", "return describe({ __proto__: @pv, y: 2 });", "true,false,2"],
  ["computedKeyIsOwn", "return describe({ ['__proto__']: pv, y: @2 });", "false,true,2"],
  ["methodIsOwn", "return describe({ __proto__() {}, y: @2 });", "false,true,2"],
  ["shorthandWithSpread", "return describe({ ...{ y: 2 }, __proto__, z: @3 });", "false,true,2"],
];

function variants(src) {
  var body = prelude + src;
  var out = [];
  out.push(["generator", () => GeneratorFunction(body.replace(/@/g, "yield ")), "sync"]);
  out.push(["generator with yield*", () => GeneratorFunction("if (false) yield* [];" + body.replace(/@/g, "yield ")), "sync"]);
  out.push(["async function", () => AsyncFunction(body.replace(/@/g, "await ")), "async"]);
  out.push(["async generator", () => AsyncGeneratorFunction(body.replace(/@/g, "yield ")), "asyncgen"]);
  return out;
}

function runSync(fn) {
  var it = fn();
  var r = it.next();
  while (!r.done) r = it.next(r.value);
  return Promise.resolve(r.value);
}

function runAsyncGen(fn) {
  var it = fn();
  function step(r) {
    return r.done ? r.value : it.next(r.value).then(step);
  }
  return it.next().then(step);
}

var jobs = [];
cases.forEach(function(c) {
  variants(c[1]).forEach(function(v) {
    jobs.push(function() {
      var fn = v[1]();
      var run = v[2] === "sync" ? runSync : v[2] === "async" ? function(f) { return f(); } : runAsyncGen;
      return run(fn).then(function(value) {
        assert.sameValue(value, c[2], c[0] + " (" + v[0] + ")");
      });
    });
  });
});

jobs.reduce(function(p, job) { return p.then(job); }, Promise.resolve()).then($DONE, $DONE);
