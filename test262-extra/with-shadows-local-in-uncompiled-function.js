/*---
description: A with object shadows function-local bindings, and removing it restores them
info: |
  14.11.2 Runtime Semantics: Evaluation of with
  9.1.2.1 GetIdentifierReference
  9.1.1.2.1 HasBinding

  Identifier resolution consults the object environment record before the
  enclosing function scope, so a property of the with object wins over a
  local of the same name for reads, writes and updates.
esid: sec-with-statement
flags: [noStrict]
---*/

function f(param) {
  var local = "local";
  var box = { local: "boxed", param: "boxedParam", counter: 10 };
  var counter = 0;
  with (box) {
    assert.sameValue(local, "boxed", "with property shadows var");
    assert.sameValue(param, "boxedParam", "with property shadows parameter");
    local = "written";
    counter++;
    counter += 5;
  }
  assert.sameValue(box.local, "written", "write went to the with object");
  assert.sameValue(box.counter, 16, "update went to the with object");
  assert.sameValue(local, "local", "var is untouched");
  assert.sameValue(counter, 0, "local counter untouched");
  return param;
}
assert.sameValue(f("p"), "p");
assert.sameValue(f("q"), "q", "a second activation is independent");
