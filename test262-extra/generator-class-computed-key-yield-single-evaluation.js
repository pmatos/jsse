/*---
description: >
  A class declaration's computed method key inside a generator body is a
  real generator-suspension point: ClassDefinitionEvaluation evaluates a
  computed ClassElementName in the enclosing scope, not inside a nested
  function/class boundary (spec 15.7.14). Resuming the generator past that
  yield must not re-run statements that already executed and must not
  re-evaluate the computed key expression.
esid: sec-generator-function-definitions-runtime-semantics-evaluation
info: |
  GeneratorResume (27.5.3) resumes a suspended generator exactly once past
  its suspension point.

  Runtime Semantics: ClassDefinitionEvaluation (15.7.14) evaluates each
  ClassElementName's computed PropertyName exactly once, in source order,
  as ordinary Evaluation in the same execution context as the enclosing
  generator.
features: [generators, computed-property-names]
---*/

var beforeClassRuns = 0;
var keyEvaluations = 0;

function computeKey() {
  keyEvaluations++;
  return "computed-key";
}

function* g() {
  beforeClassRuns++;
  class C {
    [yield computeKey()]() {
      return 1;
    }
  }
  assert.sameValue(
    typeof C.prototype["computed-key"],
    "function",
    "the computed method is installed under the yielded key"
  );
  assert.sameValue(C.prototype["computed-key"](), 1);
}

var it = g();

var first = it.next();
assert.sameValue(
  first.value,
  "computed-key",
  "the yield inside the computed key suspends with the key's value"
);
assert.sameValue(first.done, false);
assert.sameValue(
  beforeClassRuns,
  1,
  "the statement preceding the class runs exactly once before suspending"
);
assert.sameValue(
  keyEvaluations,
  1,
  "the computed key expression runs exactly once before suspending"
);

var second = it.next("computed-key");
assert.sameValue(
  second.done,
  true,
  "the generator completes after resuming past the suspended class"
);
assert.sameValue(
  beforeClassRuns,
  1,
  "resuming does not replay the statement preceding the class"
);
assert.sameValue(
  keyEvaluations,
  1,
  "resuming does not re-evaluate the computed key expression"
);
