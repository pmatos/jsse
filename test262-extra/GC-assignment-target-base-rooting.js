// Every assignment form that captures a member Reference keeps that
// Reference's base (and, for a primitive base, its ToObject result) alive
// while the remaining steps run user code. Losing the base turns PutValue into
// a silent no-op instead of reaching the inherited setter.
//
// Spec: ECMAScript 2026,
//   sec-assignment-operators-runtime-semantics-evaluation (logical assignment
//   evaluates the RHS after capturing the left Reference),
//   sec-runtime-semantics-forin-div-ofbodyevaluation (the loop target
//   Reference is evaluated, then PutValue is performed with the next value).

function allocatingRhs() {
  var values = [];
  for (var i = 0; i < 20000; i++) {
    values.push({ index: i });
  }
  return 42;
}

function allocatingKey() {
  var values = [];
  for (var i = 0; i < 20000; i++) {
    values.push({ index: i });
  }
  return "value";
}

// Logical assignment: the base is captured, the getter short-circuits to
// "assign", then the allocating RHS runs before the write-back.
var logicalObserved;
var logicalPrototype = {
  get value() {
    return 0;
  },
  set value(v) {
    logicalObserved = v;
  },
};
Object.create(logicalPrototype).value ||= allocatingRhs();
if (logicalObserved !== 42) {
  throw new Test262Error("logical assignment base was not preserved across RHS evaluation");
}

// Logical assignment on a primitive base: PutValue's receiver is the primitive
// and the [[Set]] holder is the ToObject wrapper, so the wrapper must survive
// the RHS too.
var primitiveObserved;
Object.defineProperty(Number.prototype, "gcRootingProbe", {
  configurable: true,
  set: function (v) {
    primitiveObserved = v;
  },
});
(7).gcRootingProbe ||= allocatingRhs();
delete Number.prototype.gcRootingProbe;
if (primitiveObserved !== 42) {
  throw new Test262Error("primitive-base logical assignment lost its ToObject wrapper");
}

// for-of member target: the base is evaluated before the allocating computed
// key expression.
var loopObserved;
var loopPrototype = {
  set value(v) {
    loopObserved = v;
  },
};
for (Object.create(loopPrototype)[allocatingKey()] of [7]) {
  // The member target is written before the (empty) body runs.
}
if (loopObserved !== 7) {
  throw new Test262Error("for-of member target base was not preserved across key evaluation");
}

// for-in member target takes the same PutValue path.
var forInObserved;
var forInPrototype = {
  set value(v) {
    forInObserved = v;
  },
};
for (Object.create(forInPrototype)[allocatingKey()] in { a: 1 }) {
  // The member target is written before the (empty) body runs.
}
if (forInObserved !== "a") {
  throw new Test262Error("for-in member target base was not preserved across key evaluation");
}
