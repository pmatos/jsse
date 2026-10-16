/*---
description: >
  Direct eval inside a declaration-free block across a yield declares var
  and function bindings in the function variable scope and let in its own
  scope, and the var-conflict check still sees an enclosing block let.
esid: sec-evaldeclarationinstantiation
flags: [noStrict]
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
cases.sloppyDirectEvalVarAndLet = () => {
  function* g() { { yield 1; eval("var ev = 7; let el = 8;"); } return typeof ev + "/" + typeof el; }
  return drain(g());
};
cases.directEvalVarConflictsWithEnclosingBlockLet = () => {
  function* g() { { let y = 1; { yield 1; try { eval("var y;"); } catch (e) { return e.name; } } } return "no-error"; }
  return drain(g());
};
cases.evalFunctionDeclInBlock = () => {
  function* g() { { yield 1; eval("function ef() { return 3; }"); } return typeof ef; }
  return drain(g());
};

assert.sameValue(cases.sloppyDirectEvalVarAndLet(), "Y:1,R:number/undefined", "sloppyDirectEvalVarAndLet");
assert.sameValue(cases.directEvalVarConflictsWithEnclosingBlockLet(), "Y:1,R:SyntaxError", "directEvalVarConflictsWithEnclosingBlockLet");
assert.sameValue(cases.evalFunctionDeclInBlock(), "Y:1,R:function", "evalFunctionDeclInBlock");
