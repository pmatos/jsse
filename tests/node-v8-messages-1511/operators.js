// V8 error positions of operator, assignment and heritage errors (#1511), run
// verbatim on Node and on `jsse --node` by
// scripts/run-node-v8-messages-1511-oracle.sh. Every case prints the error's
// name and message and its top stack frame, which carries the position.
const num = 1;
const sym = Symbol("s");
const big = 1n;
const obj = { toString: 1, valueOf: 1 };
const plain = {};
const thrower = {
  valueOf() {
    throw new Error("boom");
  },
};

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

show("inNumber", () => {
  return 1 in num;
});
show("inNested", () => {
  return "a" in { a: 1 } && "b" in num;
});
show("instanceofNumber", () => {
  return {} instanceof num;
});
show("instanceofNotCallable", () => {
  return {} instanceof {};
});
show("addSymbol", () => {
  return sym + "";
});
show("addSymbolRight", () => {
  return "" + sym;
});
show("addBigInt", () => {
  return big + num;
});
show("subBigInt", () => {
  return num - big;
});
show("mulSymbol", () => {
  let r = 2;
  return r * sym;
});
show("addLocalConstant", () => {
  let s = sym;
  return s + 1;
});
show("lessSymbol", () => {
  return sym < 1;
});
show("condJumpSymbol", () => {
  let s = sym;
  if (s < 1) {
    return 1;
  }
});
show("condJumpLocals", () => {
  let s = sym;
  let one = 1;
  while (s < one) {
    return 1;
  }
});
show("condJumpConstant", () => {
  let s = big;
  if (s >= 2) {
    return 1;
  }
  return s < "x" ? 1 : 2;
});
show("shiftBigInt", () => {
  return big >>> 1n;
});
show("divZeroBigInt", () => {
  return big / 0n;
});
show("expBigInt", () => {
  return 2n ** -1n;
});
show("chainedAdd", () => {
  return 1 + 2 + sym + 3;
});
show("multiLine", () => {
  return (
    1 +
    sym
  );
});
show("valueOfThrows", () => {
  return obj + 1;
});
class Brand {
  #x = 1;
  static has(o) {
    return #x in o;
  }
}
show("privateIn", () => {
  return Brand.has(num);
});
show("classHeritageNumber", () => {
  class B extends num {}
});
show("classHeritageObject", () => {
  class B extends plain {}
});
show(
  "valueOfFrames",
  () => {
    return 1 + thrower;
  },
  2,
);
show("inGenerator", () => {
  const g = (function* () {
    yield 1;
    return 1 in num;
  })();
  g.next();
  g.next();
});
show("inLoweredGenerator", () => {
  const g = (function* () {
    return (yield 1) + sym;
  })();
  g.next();
  g.next(1);
});
try {
  null instanceof 1;
} catch (e) {
  console.log("## topLevel");
  console.log(e.message);
  console.log(String(e.stack).split("\n")[1]?.trim());
}
