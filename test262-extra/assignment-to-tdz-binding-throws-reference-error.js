/*---
description: >
  Assigning to a `let`, `const` or `class` binding that is still in its
  temporal dead zone throws a ReferenceError object (never a bare string), for
  simple, compound and update assignment, in script top-level blocks, function
  bodies and switch case blocks.
info: |
  PutValue (6.2.5.6) calls SetMutableBinding (9.1.1.1.5) on the declarative
  Environment Record; step 3 throws a ReferenceError when the binding is not
  initialized. A compound assignment first performs GetValue on the same
  reference, which throws the same ReferenceError (9.1.1.1.6 step 2) instead
  of reporting the name as undeclared.
esid: sec-declarative-environment-records-setmutablebinding-n-v-s
---*/

var thrown;

function check(label, fn) {
  thrown = undefined;
  try {
    fn();
  } catch (e) {
    thrown = e;
  }
  assert.sameValue(typeof thrown, "object", label + ": thrown value type");
  assert.sameValue(
    Object.getPrototypeOf(thrown),
    ReferenceError.prototype,
    label + ": is a ReferenceError"
  );
}

check("block =", function () { { a = 1; let a; } });
check("block +=", function () { { a += 1; let a; } });
check("block ++", function () { { a++; let a; } });
check("block ||=", function () { { a ||= 1; let a; } });
check("const =", function () { { a = 1; const a = 2; } });
check("class =", function () { { a = 1; class a {} } });
check("switch =", function () { switch (1) { case 1: a = 1; case 2: let a = 2; } });
check("switch +=", function () { switch (1) { case 1: a += 1; case 2: let a = 2; } });
check("closure =", function () { var f = function () { a = 1; }; f(); let a; });
check("closure +=", function () { var f = function () { a += 1; }; f(); let a; });
check("for-head =", function () { for (let i = (j = 1, 0), j; i < 1; i++); });
check("for-of target", function () { { for (a of [1]); let a; } });
check("destructuring", function () { { [a] = [1]; let a; } });

thrown = undefined;
try {
  { a = 1; let a; }
} catch (e) {
  thrown = e;
}
assert.sameValue(typeof thrown, "object", "script block =: thrown value type");
assert.sameValue(Object.getPrototypeOf(thrown), ReferenceError.prototype, "script block =");

thrown = undefined;
try {
  { a += 1; let a; }
} catch (e) {
  thrown = e;
}
assert.sameValue(typeof thrown, "object", "script block +=: thrown value type");
assert.sameValue(Object.getPrototypeOf(thrown), ReferenceError.prototype, "script block +=");
assert.sameValue(thrown.message, "Cannot access 'a' before initialization");

thrown = undefined;
try {
  switch (1) { case 1: a += 1; case 2: let a = 2; }
} catch (e) {
  thrown = e;
}
assert.sameValue(thrown.message, "Cannot access 'a' before initialization", "script switch +=");
