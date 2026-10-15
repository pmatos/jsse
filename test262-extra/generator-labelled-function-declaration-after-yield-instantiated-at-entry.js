/*---
description: >
  A labelled function declaration after a yield in a generator body is
  instantiated at function entry.
esid: sec-functiondeclarationinstantiation
info: |
  LabelledStatement function declarations are among the body's
  VarScopedDeclarations (TopLevelVarScopedDeclarations), so
  FunctionDeclarationInstantiation initializes them before the body runs.
  Labelled function declarations are sloppy-mode only (Annex B.3.2).

  Regression test for issue #1032.
flags: [noStrict]
---*/

function* labelled() {
  yield typeof s;
  l: function s() {}
}
assert.sameValue(labelled().next().value, "function");
