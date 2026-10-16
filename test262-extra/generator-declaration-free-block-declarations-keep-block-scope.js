/*---
description: >
  A block that directly declares a let, const, class or function binding
  keeps its own declarative environment when the generator state machine
  lowers it, and a closure captures the enclosing frame of an elided block
  unchanged. The block that declares nothing must not change which scope
  any binding lands in.
esid: sec-block-runtime-semantics-evaluation
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
cases.strictBlockFunctionStaysBlockScoped = () => {
  function* g() { "use strict"; { yield 1; function f() { return 1; } } return typeof f; }
  return drain(g());
};
cases.sloppyAnnexBFunctionLeaks = () => {
  function* g() { { yield 1; function f() { return 1; } } return typeof f; }
  return drain(g());
};
cases.classInBlock = () => {
  function* g() { { yield 1; class C {} } return typeof C; }
  return drain(g());
};
cases.letShadowInNestedBlock = () => {
  function* g() { let x = "outer"; { yield x; { let x = "inner"; yield x; } yield x; } return x; }
  return drain(g());
};
cases.letAfterYieldInBlock = () => {
  function* g() { let x = "outer"; { yield x; let x2 = "blk"; yield x2; } return typeof x2; }
  return drain(g());
};
cases.tdzInBlock = () => {
  function* g() { let x = 1; { yield 1; try { x; } catch (e) { yield e.name; } let x = 2; } }
  return drain(g());
};
cases.nestedElidedThenDeclaring = () => {
  function* g() { { { yield 1; { let a = 1; yield a; const b = a + 1; yield b; } } yield typeof a; } }
  return drain(g());
};
cases.labelledFunctionKeepsBlockScope = () => {
  function* g() { { yield 1; lf: function lfn() {} } return typeof lfn; }
  return drain(g());
};
cases.closureCapturesLoopLet = () => {
  function* g() { const fs = []; for (let i = 0; i < 3; i++) { { yield i; fs.push(() => i); } } return fs.map(f => f()).join("|"); }
  return drain(g());
};
cases.elidedBlockInForOfClosures = () => {
  function* g() { const fs = []; for (const x of [1, 2, 3]) { { yield x; fs.push(() => x); } } return fs.map(f => f()).join("|"); }
  return drain(g());
};
cases.closureInElidedBlockOverFunctionVar = () => {
  function* g() { var fs = []; var j = 0; while (j < 3) { { yield j; fs.push(() => j); } j++; } return fs.map(f => f()).join("|"); }
  return drain(g());
};
cases.catchParamThroughElidedBlock = () => {
  function* g() { try { throw "P"; } catch (e) { { yield e; } { yield (() => e)(); } } }
  return drain(g());
};

assert.sameValue(cases.strictBlockFunctionStaysBlockScoped(), "Y:1,R:undefined", "strictBlockFunctionStaysBlockScoped");
assert.sameValue(cases.sloppyAnnexBFunctionLeaks(), "Y:1,R:function", "sloppyAnnexBFunctionLeaks");
assert.sameValue(cases.classInBlock(), "Y:1,R:undefined", "classInBlock");
assert.sameValue(cases.letShadowInNestedBlock(), "Y:outer,Y:inner,Y:outer,R:outer", "letShadowInNestedBlock");
assert.sameValue(cases.letAfterYieldInBlock(), "Y:outer,Y:blk,R:undefined", "letAfterYieldInBlock");
assert.sameValue(cases.tdzInBlock(), "Y:1,Y:ReferenceError,R:undefined", "tdzInBlock");
assert.sameValue(cases.nestedElidedThenDeclaring(), "Y:1,Y:1,Y:2,Y:undefined,R:undefined", "nestedElidedThenDeclaring");
assert.sameValue(cases.labelledFunctionKeepsBlockScope(), "Y:1,R:function", "labelledFunctionKeepsBlockScope");
assert.sameValue(cases.closureCapturesLoopLet(), "Y:0,Y:1,Y:2,R:0|1|2", "closureCapturesLoopLet");
assert.sameValue(cases.elidedBlockInForOfClosures(), "Y:1,Y:2,Y:3,R:1|2|3", "elidedBlockInForOfClosures");
assert.sameValue(cases.closureInElidedBlockOverFunctionVar(), "Y:0,Y:1,Y:2,R:3|3|3", "closureInElidedBlockOverFunctionVar");
assert.sameValue(cases.catchParamThroughElidedBlock(), "Y:P,Y:P,R:undefined", "catchParamThroughElidedBlock");
