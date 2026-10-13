/*---
description: >
  Block-scoped declarations inside a compiled function body keep their
  lexical environments, temporal dead zone, per-iteration bindings and
  completion routing when the bytecode VM runs the blocks, loop heads and
  switch bodies itself instead of handing them to the tree-walker.
info: |
  jsse issue #958: the VM instantiates BlockDeclarationInstantiation at block
  entry (a block environment per execution), keeps one environment for the
  whole of a `for (let ...)` loop unless an iteration's scope can be
  captured, and unwinds those environments for break, continue, return and
  throw. The observable scoping must be that of the tree-walker.

  BlockDeclarationInstantiation (14.2.3), for statement evaluation (14.7.4.2),
  CreatePerIterationEnvironment (14.7.4.4), switch statement evaluation
  (14.12.4).
esid: sec-blockdeclarationinstantiation
flags: [noStrict]
includes: [compareArray.js]
---*/

function forLetClosuresSeePerIterationBindings() {
  var fns = [];
  for (let i = 0; i < 3; i++) {
    fns.push(function () { return i; });
  }
  return fns.map(function (f) { return f(); });
}
assert.compareArray(forLetClosuresSeePerIterationBindings(), [0, 1, 2]);

function forLetClosureCreatedInUpdateSeesTheNextCopy() {
  var fns = [];
  for (let i = 0; i < 3; fns.push(function () { return i; }), i++) {}
  return fns.map(function (f) { return f(); });
}
assert.compareArray(forLetClosureCreatedInUpdateSeesTheNextCopy(), [1, 2, 3]);

function forLetClosureCreatedInInitializerKeepsTheHeadBinding() {
  var seen = [];
  for (let i = 0, g = function () { return i; }; i < 2; i++) {
    seen.push(g());
  }
  return seen;
}
assert.compareArray(forLetClosureCreatedInInitializerKeepsTheHeadBinding(), [0, 0]);

function forLetWithoutCapturesStillMutatesItsOwnBinding() {
  var sum = 0;
  for (let i = 0; i < 5; i++) {
    const doubled = i * 2;
    sum += doubled;
  }
  return sum;
}
assert.sameValue(forLetWithoutCapturesStillMutatesItsOwnBinding(), 20);

function blockLexicalIsFreshPerExecution() {
  var fns = [];
  for (var i = 0; i < 3; i++) {
    let count = i * 10;
    fns.push(function () { return ++count; });
  }
  return fns.map(function (f) { return f() + f(); });
}
assert.compareArray(blockLexicalIsFreshPerExecution(), [3, 23, 43]);

function blockTdzReadWriteAndTypeof() {
  var seen = [];
  {
    try { seen.push(x); } catch (e) { seen.push(e.name); }
    try { x = 1; } catch (e) { seen.push(e.name); }
    try { seen.push(typeof x); } catch (e) { seen.push(e.name); }
    let x = 2;
    seen.push(x);
  }
  return seen;
}
assert.compareArray(
  blockTdzReadWriteAndTypeof(),
  ["ReferenceError", "ReferenceError", "ReferenceError", 2]
);

function forLetHeadReadingItsOwnBindingThrows() {
  try {
    for (let i = i; ;) { break; }
  } catch (e) {
    return e.name;
  }
  return "none";
}
assert.sameValue(forLetHeadReadingItsOwnBindingThrows(), "ReferenceError");

function constHeadBindingIsImmutable() {
  for (const i = 0; i < 1;) {
    try { i++; } catch (e) { return e.name; }
  }
  return "none";
}
assert.sameValue(constHeadBindingIsImmutable(), "TypeError");

function blockLexicalShadowsAParameterOnlyInsideTheBlock(a) {
  var seen = [];
  { let a = 10; a++; seen.push(a); }
  for (let a = 0; a < 2; a++) { seen.push(a); }
  seen.push(a);
  return seen;
}
assert.compareArray(blockLexicalShadowsAParameterOnlyInsideTheBlock(7), [11, 0, 1, 7]);

function labeledBreakAndContinueLeaveNestedBlocks() {
  var seen = [];
  outer: for (let i = 0; i < 4; i++) {
    for (let j = 0; j < 4; j++) {
      let k = i * 10 + j;
      if (j === 1) continue outer;
      if (i === 3) break outer;
      seen.push(k);
    }
  }
  return seen;
}
assert.compareArray(labeledBreakAndContinueLeaveNestedBlocks(), [0, 10, 20]);

function handedOffBreakAndContinueLeaveBlockEnvironments() {
  var seen = [];
  for (let i = 0; i < 5; i++) {
    let x = i;
    try {
      if (i === 1) continue;
      if (i === 3) break;
      seen.push(x);
    } finally {
      seen.push("f");
    }
  }
  var later = 0;
  for (let k = 0; k < 2; k++) { later += k; }
  seen.push(later);
  return seen;
}
assert.compareArray(
  handedOffBreakAndContinueLeaveBlockEnvironments(),
  [0, "f", "f", 2, "f", "f", 1]
);

function find(limit) {
  for (let i = 0; i < 5; i++) {
    let q = i + limit;
    if (q > 3) {
      const doubled = q * 2;
      return doubled;
    }
    if (q < 0) throw new Error("negative " + q);
  }
  return -1;
}
assert.sameValue(find(0), 8, "return from inside a loop's blocks");
assert.sameValue(find(10), 20);
assert.throws(Error, function () { find(-5); }, "throw from inside a loop's blocks");
assert.sameValue(find(0), 8, "the frame is unaffected by the earlier exits");

function switchCaseBlockHasTdzAndSelectorsRunInside() {
  var seen = [];
  function pick(v) {
    switch (v) {
      case 1:
        let a = "one";
        seen.push(a);
      case 2:
        try { seen.push(a); } catch (e) { seen.push(e.name); }
        break;
      case 3: {
        const b = "three";
        seen.push(b);
      }
    }
  }
  pick(1);
  pick(2);
  pick(3);
  return seen;
}
assert.compareArray(
  switchCaseBlockHasTdzAndSelectorsRunInside(),
  ["one", "one", "ReferenceError", "three"]
);

function switchCaseLexicalDoesNotLeak() {
  var y = "outer";
  switch (1) {
    case 1:
      let y = "inner";
      break;
  }
  return y;
}
assert.sameValue(switchCaseLexicalDoesNotLeak(), "outer");

function blockClassAndDestructuringBindInTheBlock(o) {
  var seen = [];
  {
    class A { m() { return "m"; } }
    const { p, q } = o;
    let [x, y] = [p + 1, q + 1];
    seen.push(new A().m(), x + y);
  }
  seen.push(typeof A, typeof p);
  return seen;
}
assert.compareArray(
  blockClassAndDestructuringBindInTheBlock({ p: 1, q: 2 }),
  ["m", 5, "undefined", "undefined"]
);

function blockFunctionDeclarationsAreHoistedAtBlockEntry() {
  var seen = [];
  {
    seen.push(typeof g);
    function g() { return 1; }
    seen.push(g());
  }
  seen.push(typeof g);
  return seen;
}
assert.compareArray(
  blockFunctionDeclarationsAreHoistedAtBlockEntry(),
  ["function", 1, "function"]
);

function nestedBlocksShadowInnermostFirst() {
  let x = 1;
  var seen = [];
  {
    let x = 2;
    {
      let x = 3;
      {
        x += 1;
      }
      seen.push(x);
    }
    seen.push(x);
  }
  seen.push(x);
  return seen;
}
assert.compareArray(nestedBlocksShadowInnermostFirst(), [4, 2, 1]);

function recursionKeepsBlockEnvironmentsApart(n) {
  if (n === 0) return 0;
  {
    let below = recursionKeepsBlockEnvironmentsApart(n - 1);
    return below + n;
  }
}
assert.sameValue(recursionKeepsBlockEnvironmentsApart(10), 55);

function directEvalSeesBlockBindings() {
  {
    let x = 3;
    return eval("x + 1");
  }
}
assert.sameValue(directEvalSeesBlockBindings(), 4);
