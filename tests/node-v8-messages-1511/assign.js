// V8 error positions of assignment, update, delete, new-chain and
// optional-chain errors (#1511 stage 2), run verbatim on Node and on
// `jsse --node` by scripts/run-node-v8-messages-1511-oracle.sh.
const c = 1;
const sym = Symbol("s");
const o = { x: null, a: undefined, k: "k" };

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

show("constAssign", () => {
  c = 2;
});
show("constAssignExpr", () => {
  return c = 2;
});
show("constCompound", () => {
  c += 2;
});
show("constPostfix", () => {
  c++;
});
show("constPrefix", () => {
  ++c;
});
show("constPostfixValue", () => {
  return c--;
});
show("constAssignAfterCall", () => {
  String(1);
  c = 2;
});
show("constLocal", () => {
  const d = 1;
  d = 2;
});
show("constLocalCompound", () => {
  const d = 1;
  d *= 2;
});
show("constLocalUpdate", () => {
  const d = 1;
  d++;
});
show("constClosureCall", () => {
  const d = 1;
  (() => {
    d = 3;
  })();
});
show("constMultiLine", () => {
  const d = 1;
  d =
    2 + 3;
});
show("compoundNullDot", () => {
  o.x.y += 1;
});
show("compoundNullElement", () => {
  o.x[o.k] += 1;
});
show("compoundNullBase", () => {
  o.x.y.z += 1;
});
show("assignNullDot", () => {
  o.x.y = 1;
});
show("updateNullDot", () => {
  o.x.y++;
});
show("updatePrefixNullDot", () => {
  ++o.x.y;
});
show("updateNullElement", () => {
  o.x[o.k]--;
});
show("deleteNullMember", () => {
  delete o.x.y;
});
show("deleteUndefinedElement", () => {
  return delete o.a[o.k];
});
show("newChain", () => {
  new o.a.b.c();
});
show("newChainDepth", () => {
  new o.x.y();
});
show("optionalChain", () => {
  return o?.a.b;
});
show("optionalChainElement", () => {
  return o?.a[o.k];
});
show("optionalChainCall", () => {
  return o?.a.b();
});
show("optionalChainLong", () => {
  return o.a?.b.c ?? o?.x.y.z;
});
