/*---
description: >
  break, continue and labelled break leave a declaration-free block that
  holds a yield to the correct state, in loops, switch cases and if
  branches, including empty blocks.
esid: sec-block-runtime-semantics-evaluation
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
cases.labelledBlockBreak = () => {
  function* g() { l: { yield 1; if (true) break l; yield 2; } yield 3; }
  return drain(g());
};
cases.labelledBlockInLoopContinue = () => {
  function* g() { outer: for (let i = 0; i < 3; i++) { { yield i; if (i === 1) continue outer; yield "after" + i; } } }
  return drain(g());
};
cases.breakAndContinueOutOfElided = () => {
  function* g() { let n = 0; while (true) { { yield n; if (++n > 2) break; continue; } } do { { yield "d" + n; n--; if (n > 1) continue; } } while (n > 1); }
  return drain(g());
};
cases.switchCaseBlock = () => {
  function* g(k) { switch (k) { case 1: { yield "a"; } case 2: { yield "b"; break; } default: { yield "d"; } } }
  return drain(g(1)) + ";" + drain(g(3));
};
cases.elidedBlocksInBothBranches = () => {
  function* g(c) { if (c) { { yield 1; } } else { { yield 2; } } yield 3; }
  return drain(g(true)) + ";" + drain(g(false));
};
cases.emptyAndEmptyStatementBlocks = () => {
  function* g() { yield 0; {} { ; } { yield 1; } {} yield 2; }
  return drain(g());
};

assert.sameValue(cases.labelledBlockBreak(), "Y:1,Y:3,R:undefined", "labelledBlockBreak");
assert.sameValue(cases.labelledBlockInLoopContinue(), "Y:0,Y:after0,Y:1,Y:2,Y:after2,R:undefined", "labelledBlockInLoopContinue");
assert.sameValue(cases.breakAndContinueOutOfElided(), "Y:0,Y:1,Y:2,Y:d3,Y:d2,R:undefined", "breakAndContinueOutOfElided");
assert.sameValue(cases.switchCaseBlock(), "Y:a,Y:b,R:undefined;Y:d,R:undefined", "switchCaseBlock");
assert.sameValue(cases.elidedBlocksInBothBranches(), "Y:1,Y:3,R:undefined;Y:2,Y:3,R:undefined", "elidedBlocksInBothBranches");
assert.sameValue(cases.emptyAndEmptyStatementBlocks(), "Y:0,Y:1,Y:2,R:undefined", "emptyAndEmptyStatementBlocks");
