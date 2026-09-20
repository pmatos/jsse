/*---
description: >
  In an async function whose `switch` or loop contains an `await`, a
  suspension-free `try` statement whose `break` or `continue` leaves the
  enclosing clause or loop must run its finalizer and then perform the jump,
  including when an enclosing `try/finally` that does await must also run its
  finalizer.
esid: sec-try-statement-runtime-semantics-evaluation
info: |
  TryStatement : try Block Finally

  If F is a normal completion, set F to B: the `break`/`continue` completion
  of the block is the completion of the TryStatement, and it ends
  CaseBlockEvaluation (sec-runtime-semantics-caseblockevaluation) or the
  iteration (sec-loopcontinues).
flags: [async]
includes: [asyncHelpers.js]
features: [async-functions, async-iteration]
---*/

async function switchTry(x) {
  var l = [];
  switch (x) {
    case 1: try { break; } finally { l.push("f"); }
    case 2: l.push("two"); break;
    case 3: await 0;
  }
  return l.join();
}

async function forBreak() {
  var l = [];
  for (var i = 0; i < 5; i++) {
    try { if (i == 2) break; } finally { l.push("f" + i); }
    await i;
  }
  return l.join();
}

async function forContinue() {
  var l = [];
  for (var i = 0; i < 4; i++) {
    try { if (i == 1) continue; } finally { l.push("f" + i); }
    await i;
    l.push("b" + i);
  }
  return l.join();
}

async function labeledBreak() {
  var l = [];
  outer: for (var i = 0; i < 3; i++) {
    for (var j = 0; j < 3; j++) {
      try { if (i == 1 && j == 1) break outer; } finally { l.push(i + "" + j); }
      await j;
    }
  }
  return l.join();
}

async function insideAwaitingTry() {
  var l = [];
  for (var i = 0; i < 4; i++) {
    try {
      await 0;
      try { if (i == 1) break; } finally { l.push("inner" + i); }
      l.push("body" + i);
    } finally {
      await 0;
      l.push("outer" + i);
    }
  }
  return l.join();
}

async function forAwaitBreak() {
  var l = [];
  for await (var v of [0, 1, 2, 3]) {
    try { if (v == 2) break; } finally { l.push("f" + v); }
    await v;
  }
  return l.join();
}

async function forAwaitContinue() {
  var l = [];
  for await (var v of [0, 1, 2, 3]) {
    try { if (v == 1) continue; } finally { l.push("f" + v); }
    await v;
    l.push("b" + v);
  }
  return l.join();
}

asyncTest(async function() {
  assert.sameValue(await switchTry(1), "f", "switch, try + break");
  assert.sameValue(await switchTry(2), "two", "case 2");
  assert.sameValue(await switchTry(3), "", "awaiting case");
  assert.sameValue(await forBreak(), "f0,f1,f2", "for + break");
  assert.sameValue(await forContinue(), "f0,b0,f1,f2,b2,f3,b3", "for + continue");
  assert.sameValue(await labeledBreak(), "00,01,02,10,11", "break out of a labeled outer loop");
  assert.sameValue(
    await insideAwaitingTry(),
    "inner0,body0,outer0,inner1,outer1",
    "the enclosing awaiting finalizer still runs"
  );
  assert.sameValue(await forAwaitBreak(), "f0,f1,f2", "for await + break");
  assert.sameValue(await forAwaitContinue(), "f0,b0,f1,f2,b2,f3,b3", "for await + continue");
});
