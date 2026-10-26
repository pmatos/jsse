/*---
description: >
  Bare names read, written, updated, called and typeof'd inside block scopes of
  a script resolve exactly as before: a name an inner block declares shadows
  the global one, a global lexical is read and written through the declarative
  record (const and TDZ still throw), and a lexical declared by a later script
  shadows a global object property the loop already warmed.
info: |
  ResolveBinding (sec-resolvebinding) walks the running execution context's
  LexicalEnvironment outward; Global Environment Record HasBinding
  (sec-global-environment-records-hasbinding-n) consults the declarative record
  before the object record.
esid: sec-resolvebinding
flags: [noStrict]
---*/

let total = 0;
const LIMIT = 50;
var viaVar = 1;
globalThis.prop = 1;
function callee(n) { return n + 1; }

for (let i = 0; i < LIMIT; i++) {
  let sq = (i * i) & 0xff;
  total = (total + sq) | 0;
  total += 1;
  total++;
  viaVar += 1;
  prop = prop + 1;
  assert.sameValue(typeof total, "number");
  assert.sameValue(typeof missing, "undefined");
  assert.sameValue(callee(i), i + 1);
}
assert.sameValue(total, 5197, "total after loop");
assert.sameValue(viaVar, LIMIT + 1, "global var written in block");
assert.sameValue(globalThis.viaVar, LIMIT + 1, "global var mirrored on the global object");
assert.sameValue(prop, LIMIT + 1, "global object property written in block");

for (let i = 0; i < 3; i++) {
  let total = 100;
  {
    total += i;
    assert.sameValue(total, 100 + i, "inner declaration shadows the global lexical");
  }
}
assert.sameValue(total, 5197, "global lexical untouched by shadowing blocks");

for (let i = 0; i < 3; i++) {
  assert.throws(TypeError, function() { LIMIT = i; }, "const assignment in a function");
  assert.throws(TypeError, () => { { LIMIT = i; } }, "const assignment in a nested block");
}
let constAssignCaught = 0;
for (let i = 0; i < 3; i++) {
  try { LIMIT = i; } catch (e) { if (e instanceof TypeError) constAssignCaught++; }
  try { LIMIT += 1; } catch (e) { if (e instanceof TypeError) constAssignCaught++; }
  try { LIMIT++; } catch (e) { if (e instanceof TypeError) constAssignCaught++; }
}
assert.sameValue(constAssignCaught, 9, "const assignment throws at block sites");
assert.sameValue(LIMIT, 50);

let tdzCaught = 0;
for (let i = 0; i < 3; i++) {
  try { later; } catch (e) { if (e instanceof ReferenceError) tdzCaught++; }
  try { later = 1; } catch (e) { if (e instanceof ReferenceError) tdzCaught++; }
  try { typeof later; } catch (e) { if (e instanceof ReferenceError) tdzCaught++; }
}
assert.sameValue(tdzCaught, 9, "TDZ read, write and typeof throw at block sites");
let later = 7;
for (let i = 0; i < 2; i++) {
  later += 1;
}
assert.sameValue(later, 9);

globalThis.warm = 10;
let seen = [];
for (let i = 0; i < 4; i++) {
  if (i === 2) $262.evalScript("let warm = 20;");
  seen.push(warm);
  warm = warm + 1;
}
assert.sameValue(seen.join(), "10,11,20,21", "later script's lexical shadows the warmed property");
assert.sameValue(globalThis.warm, 12, "the global object property keeps its own value");

{
  let blockOnly = 1;
  for (let i = 0; i < 3; i++) {
    blockOnly += i;
  }
  assert.sameValue(blockOnly, 4);
}
assert.sameValue(typeof blockOnly, "undefined");
