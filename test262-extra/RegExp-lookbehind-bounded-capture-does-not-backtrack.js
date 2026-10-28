/*---
esid: sec-runtime-semantics-compileassertion-node
description: >
  A lookbehind evaluates right-to-left and never backtracks into its captures,
  including captures of bounded variable width such as {1,3}.
info: |
  RS: CompileAssertion
  Assertion :: (?<= Disjunction )
  3. Let m be CompileSubpattern of Disjunction with arguments rer and backward.
  ...
  e. If r is failure, return failure.
  (The lookbehind is a single non-backtracking step once m succeeds.)
features: [regexp-lookbehind]
---*/

assert.sameValue("abcdbc".match(/(?<=([abc]{1,3})).\1/), null, "{1,3} capture");
assert.sameValue("abcdbc".match(/(?<=([abc]+)).\1/), null, "+ capture");
