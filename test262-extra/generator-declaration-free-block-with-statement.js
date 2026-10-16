/*---
description: >
  A block under a with statement resolves and assigns names through the
  with object across a yield, and a closure created in it keeps the with
  scope.
esid: sec-with-statement-runtime-semantics-evaluation
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
cases.withObjectProperty = () => {
  const o = { x: 1 };
  function* g() { with (o) { { yield x; x = 2; var v = 3; } } return o.x + "/" + o.v + "/" + typeof v; }
  return drain(g());
};
cases.withClosure = () => {
  const o = { x: "o" };
  function* g() { let f; with (o) { { yield 1; f = () => x; } } o.x = "changed"; return f(); }
  return drain(g());
};

assert.sameValue(cases.withObjectProperty(), "Y:1,R:2/undefined/number", "withObjectProperty");
assert.sameValue(cases.withClosure(), "Y:1,R:changed", "withClosure");
