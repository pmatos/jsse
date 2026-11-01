// V8 error positions that come from the statement an error is thrown in and
// from the identifier it reads (#1511 stage 4), run verbatim on Node and on
// `jsse --node` by scripts/run-node-v8-messages-1511-oracle.sh.
const sym = Symbol("s");
const num = 1;
const n = null;
const m = [];
const o = {};
const f = function () {};

function show(name, fn, frames = 1) {
  try {
    fn();
    console.log("## " + name + ": no error");
  } catch (e) {
    console.log("## " + name);
    console.log(e.name + ": " + e.message);
    for (const line of String(e.stack).split("\n").slice(1, 1 + frames)) {
      console.log(line.trim());
    }
  }
}

// The statement start: a read of a missing binding, the call of one, and
// errors no sub-expression has a position for.
show("undeclaredOperand", () => {
  return x + 1;
});
show("undeclaredCall", () => {
  x();
});
show("undeclaredNew", () => {
  new x();
});
show("undeclaredAfterStatement", () => {
  String(1);
  x + 1;
});
show("undeclaredInIf", () => {
  if (x) {
  }
});
show("undeclaredInElseIf", () => {
  if (0) {
  } else if (x) {
  }
});
show("undeclaredInSwitch", () => {
  switch (x) {
  }
});
show("undeclaredInThrow", () => {
  throw x;
});
show("undeclaredInBlock", () => {
  {
    x;
  }
});
show("undeclaredInLabel", () => {
  l: x;
});
show("undeclaredInTry", () => {
  try {
    x;
  } finally {
  }
});
show("undeclaredInCatch", () => {
  try {
    throw 1;
  } catch (e) {
    x;
  }
});
show("undeclaredUpdate", () => {
  typeof x;
  x++;
});
show("undeclaredStrictAssign", () => {
  "use strict";
  x = 1;
});
show("deleteNull", () => {
  delete null.x;
});
show("deleteNullAfterStatement", () => {
  1;
  delete null.x;
});
show("deleteNullReturn", () => {
  return delete null.x;
});
show("nullishCompound", () => {
  n.y += 1;
});
show("nullishUpdate", () => {
  n[0]--;
});
show("nullishDelete", () => {
  delete n.y;
});
show("nullishAfterDeclaration", () => {
  let q = 1;
  n.y += 1;
});
show("symbolLeft", () => {
  let s = sym;
  s += 1;
});
show("symbolLeftBigInt", () => {
  let s = sym;
  s += 1n;
});
show("symbolUnary", () => {
  +sym;
});
show("symbolTemplate", () => {
  `${sym}`;
});
show("symbolBinary", () => {
  1 + sym;
});

// The identifier: an uninitialised or missing binding that is not the first
// thing its statement evaluates is reported where it is read.
show("symbolRight", () => {
  let s = 1;
  s += sym;
});
show("symbolRightBigInt", () => {
  let s = 1n;
  s += sym;
});
show("tdzInitializer", () => {
  let z = z;
});
show("tdzInitializerSum", () => {
  let z = 1 + z;
});
show("tdzReturnSum", () => {
  return 1 + z;
  let z;
});
show("tdzArgument", () => {
  f(1, z);
  let z;
});
show("tdzMemberStore", () => {
  o.p = z;
  let z;
});
show("tdzArray", () => {
  [0, z];
  let z;
});
show("tdzFirstInStatement", () => {
  z + 1;
  let z;
});
show("tdzFirstInIf", () => {
  if (z) {
  }
  let z;
});
show("tdzFirstInReturn", () => {
  return z;
  let z;
});
show("tdzFirstAssignment", () => {
  let y;
  y = z;
  let z;
});
show("tdzClosure", () => {
  const g = () => later;
  g();
});
show("tdzDeclarationWithCall", () => {
  const w = f() + z;
  let z;
});
show("undeclaredInitializerSum", () => {
  var a = 1 + x;
});
show("undeclaredCallee", () => {
  1 + x();
});
show("conciseArrowSum", () => x + 1);
show("conciseArrowCall", () => x());
show("conciseArrowIdentifier", () => x);

// Suspended generators keep the statement position of the code resumed.
show("generatorUndeclared", () => {
  function* g() {
    yield 1;
    q + (yield 2);
  }
  const it = g();
  it.next();
  it.next();
});
show("generatorTdz", () => {
  function* g() {
    yield 1;
    let a = 1 + z;
  }
  const it = g();
  it.next();
  it.next();
  let z;
});
show("generatorNullish", () => {
  function* g() {
    const t = { x: null };
    yield 1;
    t.x.y += 1;
  }
  const it = g();
  it.next();
  it.next();
});

let later = (() => 1)();
