/*---
description: >
  A `yield` inside the default of a property ahead of a trailing object
  rest, in a `for-of` head's `var` declaration pattern, suspends the
  generator once per iteration and resumes using the sent value -- the
  for-of-head desugar turns the per-iteration binding into an ordinary
  declaration, which reaches the same `Declaration`-form lowering as any
  other suspending object-rest pattern (issue #771).
esid: sec-runtime-semantics-forin-div-ofbodyevaluation
info: |
  ForIn/OfBodyEvaluation

  ...
  g. Let status be Completion(ForBindingInitialization of lhs with
     nextValue and iterationEnv).
  ...

  Nothing in BindingInitialization restricts a `yield` expression from
  appearing in a property default, so a generator must suspend at it on
  every iteration, binding the per-iteration rest object independently each
  time.
features: [generators, destructuring-binding, object-rest]
---*/

function* g() {
  var out = [];
  for (var { a = yield 1, ...rest } of [{ c: 3 }, { a: 5, c: 6 }]) {
    out.push({ a: a, rest: rest });
  }
  return out;
}

var it = g();
var r1 = it.next();
assert.sameValue(r1.value, 1, "the first iteration's default yield suspends the generator");
assert.sameValue(r1.done, false, 'the generator has not completed the first iteration yet');

var r2 = it.next(77);
assert.sameValue(r2.done, true, 'both iterations complete (the second has no default to await)');
assert.sameValue(r2.value.length, 2, 'both iterations ran');
assert.sameValue(r2.value[0].a, 77, 'the first iteration uses the sent value for the default');
assert.sameValue(r2.value[0].rest.c, 3, "the first iteration's rest keeps its own properties");
assert.sameValue(r2.value[1].a, 5, 'the second iteration has a present property, no default runs');
assert.sameValue(r2.value[1].rest.c, 6, "the second iteration's rest keeps its own properties");
