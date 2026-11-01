// V8 error positions of class heritage errors (#1511 stage 2), run verbatim on
// Node and on `jsse --node` by scripts/run-node-v8-messages-1511-oracle.sh.
const num = 5;
function f() {
  return 5;
}

function show(name, fn) {
  try {
    fn();
    console.log("## " + name + ": no error");
  } catch (e) {
    console.log("## " + name);
    console.log(e.name + ": " + e.message);
    console.log(String(e.stack).split("\n")[1].trim());
  }
}

show("parenthesised", () => {
  class B extends (num) {}
});
show("parenthesisedCall", () => {
  class B extends (f()) {}
});
show("doubleParenthesised", () => {
  class B extends ((num)) {}
});
show("commentAndSpaces", () => {
  class B extends  /* c */ ( num ) {}
});
show("call", () => {
  class B extends f() {}
});
show("yieldHeritage", () => {
  const g = (function* () {
    class B extends (yield 1) {}
  })();
  g.next();
  g.next(5);
});
show("yieldHeritageExpression", () => {
  const g = (function* () {
    const C = class A extends (yield 1) {};
  })();
  g.next();
  g.next({});
});
const awaited = async function () {
  class B extends (await 1) {}
};
awaited().catch((e) => {
  console.log("## awaitHeritage");
  console.log(e.name + ": " + e.message);
  console.log(String(e.stack).split("\n")[1].trim());
});
