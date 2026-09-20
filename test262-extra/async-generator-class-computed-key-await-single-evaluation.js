/*---
description: >
  Async mirror of the sync-generator case: a class declaration's computed
  method key inside an async generator body is a real suspension point when
  it contains `await`. ClassDefinitionEvaluation evaluates a computed
  ClassElementName in the enclosing scope, not inside a nested
  function/class boundary (spec 15.7.14), so an `await` there suspends the
  async generator itself. Resuming past it must not re-run statements that
  already executed and must not re-evaluate the computed key expression.
esid: sec-asyncgenerator-prototype-next
info: |
  AsyncGeneratorResume resumes a suspended async generator exactly once
  past its suspension point.

  Runtime Semantics: ClassDefinitionEvaluation (15.7.14) evaluates each
  ClassElementName's computed PropertyName exactly once, in source order,
  as ordinary Evaluation in the same execution context as the enclosing
  async generator.
flags: [async]
features: [async-generators, computed-property-names]
---*/

var beforeClassRuns = 0;
var keyEvaluations = 0;

function computeKey() {
  keyEvaluations++;
  return Promise.resolve("computed-key");
}

async function* g() {
  yield "first";
  beforeClassRuns++;
  class C {
    [await computeKey()]() {
      return 1;
    }
  }
  assert.sameValue(
    typeof C.prototype["computed-key"],
    "function",
    "the computed method is installed under the awaited key"
  );
  yield "after-class";
}

async function main() {
  var it = g();

  var r1 = await it.next();
  assert.sameValue(r1.value, "first");
  assert.sameValue(r1.done, false);
  assert.sameValue(beforeClassRuns, 0, "the class has not run yet");
  assert.sameValue(keyEvaluations, 0, "the computed key has not run yet");

  var r2 = await it.next();
  assert.sameValue(r2.value, "after-class");
  assert.sameValue(r2.done, false);
  assert.sameValue(
    beforeClassRuns,
    1,
    "the statement preceding the class runs exactly once"
  );
  assert.sameValue(
    keyEvaluations,
    1,
    "the computed key expression runs exactly once"
  );

  var r3 = await it.next();
  assert.sameValue(r3.done, true);
  assert.sameValue(
    beforeClassRuns,
    1,
    "resuming past the final yield does not replay the statement preceding the class"
  );
  assert.sameValue(
    keyEvaluations,
    1,
    "resuming past the final yield does not re-evaluate the computed key expression"
  );
}

main().then($DONE, $DONE);
