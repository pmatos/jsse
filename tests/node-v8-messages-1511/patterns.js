// V8 error positions of destructuring, array pattern, for-of and spread
// errors (#1511 stage 3, #1566), run verbatim on Node and on `jsse --node` by
// scripts/run-node-v8-messages-1511-oracle.sh.
const o = {};
const n = null;
const u = undefined;
const num = 5;
function f() {}

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

show("destructureNullLiteral", () => {
  var { a } = null;
});
show("destructureVariable", () => {
  var { a } = n;
});
show("destructureUndefined", () => {
  var { a } = u;
});
show("destructureMember", () => {
  var { a } = o.p;
});
show("destructureCall", () => {
  var { a } = f();
});
show("destructureLet", () => {
  let { a } = n;
});
show("destructureConst", () => {
  const { a } = n;
});
show("destructureNoProperty", () => {
  var {} = n;
});
show("destructureRest", () => {
  var { ...r } = n;
});
show("destructureComputed", () => {
  var { [num]: b } = n;
});
show("destructureStringKey", () => {
  var { s: b, t: c } = n;
});
show("destructureKeyValue", () => {
  var {
    s: { t },
  } = { s: null };
});
show("destructureAssign", () => {
  let a;
  ({ a } = n);
});
show("destructureAssignMember", () => {
  let a;
  ({ a } = o.p);
});
show("destructureAssignKey", () => {
  let b;
  ({ s: b } = n);
});
show("destructureAssignEmpty", () => {
  ({} = n);
});
show("destructureNested", () => {
  var {
    a: { b },
  } = { a: null };
});
show("destructureNestedArray", () => {
  var [{ b }] = [null];
});
show("destructureDefault", () => {
  var { a = 1 } = n;
});
show("arrayNullLiteral", () => {
  var [a] = null;
});
show("arrayVariable", () => {
  var [a] = n;
});
show("arrayUndefined", () => {
  var [a] = u;
});
show("arrayMember", () => {
  var [a] = o.p;
});
show("arrayCall", () => {
  var [a] = f();
});
show("arrayEmpty", () => {
  var [] = n;
});
show("arrayNumber", () => {
  var [a] = num;
});
show("arrayAssign", () => {
  let a;
  [a] = n;
});
show("forOfLiteral", () => {
  for (const x of 5);
});
show("forOfVariable", () => {
  for (const x of num);
});
show("forOfNull", () => {
  for (const x of n);
});
show("forOfObject", () => {
  for (const x of o);
});
show("forOfMember", () => {
  for (const x of o.p);
});
show("forOfCall", () => {
  for (const x of f());
});
show("forOfObjectLiteral", () => {
  for (const x of {});
});
show("forOfVar", () => {
  for (var x of num);
});
show("forOfAssign", () => {
  let x;
  for (x of num);
});
show("forOfPattern", () => {
  for (const [x] of num);
});
show("forOfBlock", () => {
  String(1);
  for (const x of num) {
    String(2);
  }
});
show("spreadLiteral", () => {
  return [...5];
});
show("spreadVariable", () => {
  return [...num];
});
show("spreadObject", () => {
  return [...o];
});
show("spreadNull", () => {
  return [...n];
});
show("spreadMember", () => {
  return [...o.p];
});
show("spreadCall", () => {
  return [...f()];
});
show("spreadSecond", () => {
  return [1, ...[2], ...num];
});
show("spreadArgumentSecond", () => {
  f(1, ...[2], ...num);
});
show("spreadAfterCall", () => {
  String(1);
  return [...num];
});
