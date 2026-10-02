/*---
description: >
  A top-level (not nested in any block) `const` declarator in a generator
  body whose own initializer is the suspension point itself
  (`const x = yield;`) must still create a genuine immutable binding --
  not a plain mutable function-level temporary variable. Reassigning the
  name after the generator resumes must throw a TypeError, the same as any
  other `const` reassignment.
esid: sec-let-and-const-declarations-runtime-semantics-evaluation
info: |
  LexicalBinding : BindingIdentifier Initializer

  1. Let bindingId be StringValue of BindingIdentifier.
  2. Let lhs be ! ResolveBinding(bindingId).
  3. Let rhs be ? Evaluation of Initializer.
  4. Let value be ? GetValue(rhs).
  5. ...
  6. Perform ! InitializeReferencedBinding(lhs, value).

  ResolveBinding finds the binding already created (uninitialized) for
  bindingId; a `const` declarator creates an *immutable* binding
  regardless of whether its own Initializer happens to be a suspension
  point. A later assignment to an immutable binding throws a TypeError
  (`sec-declarative-environment-records-setmutablebinding-n-v-s`).
features: [generators]
---*/

function* g() {
  const x = yield;
  x = 2;
}

var it = g();
it.next();
assert.throws(
  TypeError,
  function () { it.next(5); },
  'reassigning a top-level const whose own initializer suspended throws TypeError'
);
