/*---
description: >
  return and throw, whether executed inside a declaration-free block or
  delivered by generator.return/throw while suspended in one, run
  try/catch/finally and iterator closing in order.
esid: sec-generator.prototype.return
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
cases.tryFinallyReturnFromBlock = () => {
  function* g() { try { { yield 1; return 5; } } finally { yield "f"; } }
  return drain(g());
};
cases.tryCatchThrowFromBlock = () => {
  function* g() { try { { yield 1; throw new Error("boom"); } } catch (e) { yield e.message; { yield "in-catch"; } } }
  return drain(g());
};
cases.returnWhileSuspendedInsideForOf = () => {
  let closed = 0;
  function* src() { try { yield 1; yield 2; } finally { closed++; } }
  function* g() { for (const x of src()) { { yield x; } } }
  const it = g(); it.next();
  const r = it.return("done");
  return JSON.stringify(r) + "/closed=" + closed;
};
cases.throwWhileSuspendedInBlock = () => {
  function* g() { try { { yield 1; } } catch (e) { yield "caught " + e; } }
  const it = g(); it.next();
  return JSON.stringify(it.throw("T")) + JSON.stringify(it.next());
};
cases.catchAndFinallyElided = () => {
  function* g() { try { throw "e"; } catch (e) { { yield e; } } finally { { yield "f"; } } }
  return drain(g());
};

assert.sameValue(cases.tryFinallyReturnFromBlock(), "Y:1,Y:f,R:5", "tryFinallyReturnFromBlock");
assert.sameValue(cases.tryCatchThrowFromBlock(), "Y:1,Y:boom,Y:in-catch,R:undefined", "tryCatchThrowFromBlock");
assert.sameValue(cases.returnWhileSuspendedInsideForOf(), "{\"value\":\"done\",\"done\":true}/closed=1", "returnWhileSuspendedInsideForOf");
assert.sameValue(cases.throwWhileSuspendedInBlock(), "{\"value\":\"caught T\",\"done\":false}{\"done\":true}", "throwWhileSuspendedInBlock");
assert.sameValue(cases.catchAndFinallyElided(), "Y:e,Y:f,R:undefined", "catchAndFinallyElided");
