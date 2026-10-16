/*---
description: >
  arguments, this, super, yield* and values sent with next() are unchanged
  inside a declaration-free block that holds a yield.
esid: sec-generator-function-definitions-runtime-semantics-evaluation
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
cases.generatorArgumentsAndThis = () => {
  function* g() { { yield arguments.length; yield this === undefined ? "u" : typeof this; } }
  return drain(g.call(undefined, 1, 2));
};
cases.newTargetAndHomeObject = () => {
  const o = { *g() { { yield typeof super.toString; } } };
  return drain(o.g());
};
cases.yieldStarInElidedBlock = () => {
  function* inner() { yield "a"; return "r"; }
  function* g() { { const unused = 0; } { const r = yield* inner(); yield r; } }
  return drain(g());
};
cases.sendValuesThroughElided = () => {
  function* g() { { const a = yield "first"; { yield "got " + a; } } }
  return drain(g(), [undefined, "S"]);
};

assert.sameValue(cases.generatorArgumentsAndThis(), "Y:2,Y:object,R:undefined", "generatorArgumentsAndThis");
assert.sameValue(cases.newTargetAndHomeObject(), "Y:function,R:undefined", "newTargetAndHomeObject");
assert.sameValue(cases.yieldStarInElidedBlock(), "Y:a,Y:r,R:undefined", "yieldStarInElidedBlock");
assert.sameValue(cases.sendValuesThroughElided(), "Y:first,Y:got S,R:undefined", "sendValuesThroughElided");
