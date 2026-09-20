/*---
description: >
  A class EXPRESSION's heritage and computed method key inside a generator
  body are real generator-suspension points, exactly like the same positions
  in a class declaration: ClassDefinitionEvaluation evaluates the heritage and
  each computed ClassElementName as ordinary Evaluation in the same execution
  context as the enclosing generator (spec 15.7.14). When such a `yield` is the
  generator's only suspension, resuming past it must not re-run statements
  that already executed and must not re-evaluate the suspending expression.
esid: sec-generator-function-definitions-runtime-semantics-evaluation
info: |
  GeneratorResume (27.5.3) resumes a suspended generator exactly once past
  its suspension point.

  Runtime Semantics: ClassDefinitionEvaluation (15.7.14) evaluates
  ClassHeritage and each ClassElementName's computed PropertyName in the
  same execution context as the enclosing generator.
features: [generators, computed-property-names]
---*/

var beforeRuns = 0;
var heritageEvaluations = 0;
var keyEvaluations = 0;

function Base() {}

function computeHeritage() {
  heritageEvaluations++;
  return Base;
}

function computeKey() {
  keyEvaluations++;
  return "computed-key";
}

function* heritageGen() {
  beforeRuns++;
  let C = class extends (yield computeHeritage()) {};
  return C;
}

var it = heritageGen();
var first = it.next();
assert.sameValue(first.done, false);
assert.sameValue(first.value, Base, "the yield in the heritage suspends with its operand");
assert.sameValue(beforeRuns, 1, "the statement preceding the class expression ran once");
assert.sameValue(heritageEvaluations, 1);

var second = it.next(Base);
assert.sameValue(second.done, true);
assert.sameValue(
  Object.getPrototypeOf(second.value),
  Base,
  "the resumed value is used as the class heritage"
);
assert.sameValue(beforeRuns, 1, "resuming does not replay the preceding statement");
assert.sameValue(heritageEvaluations, 1, "resuming does not re-evaluate the heritage operand");

beforeRuns = 0;

function* keyGen() {
  beforeRuns++;
  let C = class {
    [yield computeKey()]() {
      return 1;
    }
  };
  return C;
}

var it2 = keyGen();
var firstKey = it2.next();
assert.sameValue(firstKey.value, "computed-key");
assert.sameValue(firstKey.done, false);
assert.sameValue(beforeRuns, 1, "the statement preceding the class expression ran once");
assert.sameValue(keyEvaluations, 1);

var secondKey = it2.next("computed-key");
assert.sameValue(secondKey.done, true);
assert.sameValue(
  typeof secondKey.value.prototype["computed-key"],
  "function",
  "the computed method is installed under the yielded key"
);
assert.sameValue(beforeRuns, 1, "resuming does not replay the preceding statement");
assert.sameValue(keyEvaluations, 1, "resuming does not re-evaluate the computed key expression");

var argLog = [];

function argProbe(a, b) {
  return [a, typeof b];
}

function* argGen() {
  return argProbe(argLog.push("a"), class {
    [yield "k"]() {}
  });
}

var it3 = argGen();
var firstArg = it3.next();
assert.sameValue(
  firstArg.done,
  false,
  "a class expression used as a call argument suspends the generator in its computed key"
);
assert.sameValue(firstArg.value, "k");

var secondArg = it3.next("m");
assert.sameValue(secondArg.done, true);
assert.sameValue(secondArg.value[0], 1);
assert.sameValue(secondArg.value[1], "function", "the class expression argument is defined");
assert.sameValue(argLog.length, 1, "the preceding argument expression ran once");
