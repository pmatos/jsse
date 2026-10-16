/*---
description: >
  Annex B function-in-block hoisting through a declaration-free block that
  holds a yield stays observable exactly as without the generator
  transform.
esid: sec-block-level-function-declarations-web-legacy-compatibility-semantics
flags: [noStrict]
features: [generators]
---*/

function drain(it, sends) {
  const out = [];
  let r, k = 0;
  while (true) {
    r = it.next(sends ? sends[k++] : undefined);
    out.push(r.done ? "R:" + String(r.value) : "Y:" + String(r.value));
    if (r.done || out.length > 40) break;
  }
  return out.join(",");
}

const cases = {};
cases.annexBFunctionInNestedBlockUnderElidedBlock = () => {
  function* g() { { yield 1; { function nf() { return 1; } } } return typeof nf; }
  return drain(g());
};
cases.strictFunctionInNestedBlockUnderElidedBlock = () => {
  function* g() { "use strict"; { yield 1; { function nf() { return 1; } } } return typeof nf; }
  return drain(g());
};
cases.annexBIfFunctionUnderElidedBlock = () => {
  function* g() { { yield 1; if (true) function iff() { return 1; } } return typeof iff; }
  return drain(g());
};

assert.sameValue(cases.annexBFunctionInNestedBlockUnderElidedBlock(), "Y:1,R:function", "annexBFunctionInNestedBlockUnderElidedBlock");
assert.sameValue(cases.strictFunctionInNestedBlockUnderElidedBlock(), "Y:1,R:undefined", "strictFunctionInNestedBlockUnderElidedBlock");
assert.sameValue(cases.annexBIfFunctionUnderElidedBlock(), "Y:1,R:function", "annexBIfFunctionUnderElidedBlock");
