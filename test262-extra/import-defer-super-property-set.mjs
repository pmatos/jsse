// Tests that super[key] = value on a deferred module namespace receiver
// triggers synchronous evaluation per the import-defer proposal, while still
// rejecting the [[Set]] per §10.4.6 of the spec.
//
// Companion to (and stricter than) the upstream test262 cases
// language/import/import-defer/evaluation-triggers/trigger-{,not-}exported-string-super-property-set-exported.js
// — those wrap `new B()` in `try { } catch (_) {}` and only check that
// evaluation fired, while this test additionally asserts:
//   (1) the rejection is a TypeError (strict module code),
//   (2) the namespace's export binding is unchanged,
//   (3) a non-exported key does not create a new property,
//   (4) evaluation does not re-run on a second super-set,
//   (5) symbol-like keys still throw TypeError.
//
// Spec: https://tc39.es/proposal-defer-import-eval/ §10.4.6

globalThis.evaluations = [];

import defer * as ns from "./import-defer-super-property-set-dep.mjs";

if (globalThis.evaluations.length !== 0) {
  throw new Test262Error("import defer should not pre-evaluate");
}

// (1)/(2): exported key — triggers evaluation, throws TypeError, binding unchanged.
{
  const key = "exported";
  class A { constructor() { return ns; } }
  class B extends A {
    constructor() {
      super();
      super[key] = 14;
    }
  }
  let caught;
  try { new B(); } catch (e) { caught = e; }
  if (globalThis.evaluations.length === 0) {
    throw new Test262Error("super[exported] = should trigger deferred evaluation");
  }
  if (!(caught instanceof TypeError)) {
    throw new Test262Error("expected TypeError on strict super-set of exported, got " + caught);
  }
  if (ns.exported !== 3) {
    throw new Test262Error("ns.exported must remain 3 after rejected [[Set]], got " + ns.exported);
  }
}

// (3)/(4): non-exported key — no new property, no re-evaluation.
{
  const before = globalThis.evaluations.length;
  const key = "notExported";
  class A { constructor() { return ns; } }
  class B extends A {
    constructor() {
      super();
      super[key] = 7;
    }
  }
  let caught;
  try { new B(); } catch (e) { caught = e; }
  if (!(caught instanceof TypeError)) {
    throw new Test262Error("expected TypeError on strict super-set of notExported, got " + caught);
  }
  if ("notExported" in ns) {
    throw new Test262Error("ns.notExported must not be created on rejected [[Set]]");
  }
  if (globalThis.evaluations.length !== before) {
    throw new Test262Error("evaluation must not re-run after first trigger");
  }
}

// (5): symbol-like key — TypeError, no spurious evaluation re-run.
{
  const before = globalThis.evaluations.length;
  class A { constructor() { return ns; } }
  class B extends A {
    constructor() {
      super();
      super[Symbol.iterator] = function*() {};
    }
  }
  let caught;
  try { new B(); } catch (e) { caught = e; }
  if (!(caught instanceof TypeError)) {
    throw new Test262Error("expected TypeError on strict super-set of Symbol.iterator, got " + caught);
  }
  if (globalThis.evaluations.length !== before) {
    throw new Test262Error("symbol-like super-set must not re-evaluate");
  }
}
