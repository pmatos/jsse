/*---
description: >
  A `yield` inside the default of a property ahead of a trailing object
  rest suspends the generator exactly once and resumes using the value
  sent to `.next()`, even when the property's own getter is non-idempotent
  (returns `undefined` on its first call, a different value on any later
  call). Before this fix, the tree-walker's InlineYield replay fallback
  re-ran the whole pattern binding on resume, calling the getter a second
  time; its new, no-longer-`undefined` return discarded the sent value and
  skipped the default entirely (documented in
  docs/adr/2026-09-22-1752-yield-in-declaration-pattern-default.md,
  closed by issue #771).
esid: sec-runtime-semantics-keyedbindinginitialization
info: |
  SingleNameBinding : BindingIdentifier Initializer?

  1. Let bindingId be the StringValue of BindingIdentifier.
  2. Let lhs be ? ResolveBinding(bindingId, environment).
  3. Let v be ? GetV(value, propertyName).
  4. If Initializer is present and v is undefined, then
     ...
     b. Else,
       i. Let defaultValue be ? Evaluation of Initializer.
       ii. Set v to ? GetValue(defaultValue).
  5. If environment is undefined, return ? PutValue(lhs, v).
  6. Return ? InitializeReferencedBinding(lhs, v).

  GetV is step 3, strictly before the Initializer's own Evaluation in step 4
  -- so a correct implementation calls the getter exactly once per
  property, regardless of how many times the enclosing generator suspends
  and resumes while evaluating that property's Initializer.
features: [generators, destructuring-binding]
---*/

var calls = 0;
function* g() {
  var { a = yield 1, ...rest } = {
    get a() {
      calls += 1;
      return calls === 1 ? undefined : 42;
    },
    c: 3
  };
  return { a: a, rest: rest };
}

var it = g();
var r1 = it.next();
assert.sameValue(r1.value, 1, "the property's default yield suspends the generator");
assert.sameValue(r1.done, false, 'the generator has not completed yet');

var r2 = it.next(99);
assert.sameValue(r2.done, true, 'the generator completes on resume');
assert.sameValue(
  r2.value.a,
  99,
  'the value sent to .next() is used for the default, not discarded'
);
assert.sameValue(r2.value.rest.c, 3, 'rest still collects properties not named by the pattern');
assert.sameValue(calls, 1, "the property's getter is called exactly once, never replayed");
