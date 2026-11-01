// V8 error positions and frames of destructuring errors in a function's
// parameters (#1511 stage 3, #1566), run verbatim on Node and on `jsse --node`
// by scripts/run-node-v8-messages-1511-oracle.sh. The callee has a frame at
// the pattern, the caller one at the call.
function show(name, fn) {
  try {
    fn();
    console.log("## " + name + ": no error");
  } catch (e) {
    console.log("## " + name);
    console.log(e.name + ": " + e.message);
    for (const line of String(e.stack).split("\n").slice(1, 3)) {
      console.log(line.trim());
    }
  }
}

async function showAsync(name, fn) {
  try {
    await fn();
    console.log("## " + name + ": no error");
  } catch (e) {
    console.log("## " + name);
    console.log(e.name + ": " + e.message);
    for (const line of String(e.stack).split("\n").slice(1, 2)) {
      console.log(line.trim());
    }
  }
}

show("expression", () => {
  (function ({ a }) {})();
});
show("expressionNull", () => {
  (function ({ a }) {})(null);
});
show("second", () => {
  (function (x, { a }) {})(1);
});
show("keyValue", () => {
  (function ({ k: b }) {})();
});
show("empty", () => {
  (function ({}) {})();
});
show("computed", () => {
  (function ({ ["k"]: b }) {})();
});
show("rest", () => {
  (function ({ ...r }) {})();
});
show("arrow", () => {
  (({ a }) => a)();
});
show("arrowSecond", () => {
  ((x, { a }) => a)(1);
});
show("declaration", () => {
  function g({ a, b }) {}
  g();
});
show("method", () => {
  const o = { m({ a }) {} };
  o.m();
});
show("classMethod", () => {
  class C {
    m({ a }) {}
  }
  new C().m();
});
show("constructor", () => {
  class C {
    constructor({ a }) {}
  }
  new C();
});
show("newExpression", () => {
  function F({ a }) {}
  new F();
});
show("defaulted", () => {
  function g(x = 1, { a = 2 } = null) {}
  g();
});
show("nested", () => {
  function g({ a: { b } }) {}
  g({ a: null });
});
show("generator", () => {
  function* g({ a }) {}
  g();
});
show("tail", () => {
  function inner({ a }) {}
  function outer() {
    return inner();
  }
  outer();
});
(async () => {
  await showAsync("async", async () => {
    await (async function ({ a }) {})(null);
  });
  await showAsync("asyncArrow", async () => {
    await (async ({ a }) => a)();
  });
  await showAsync("asyncMethod", async () => {
    const o = { async m({ a }) {} };
    await o.m();
  });
})();
