/*---
description: >
  Earlier tagged-template substitution values remain live while later
  substitutions are evaluated.
info: |
  ArgumentListEvaluation passes the template object followed by each
  substitution value to EvaluateCall. Evaluating a later substitution must not
  make an earlier substitution unreachable before that call.
  ECMAScript section: sec-runtime-semantics-argumentlistevaluation.
---*/

function tag(strings) {
  return Array.prototype.slice.call(arguments, 1);
}

var values = tag`${{marker: "first"}}${($262.gc(), {marker: "second"})}`;

assert.sameValue(values[0].marker, "first");
assert.sameValue(values[1].marker, "second");
