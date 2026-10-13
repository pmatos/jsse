/*---
description: >
  The completion value of a Script is the value of its last value-producing
  statement, whichever statements are lowered natively and whichever run as
  tree-walker units (try, switch, labelled blocks, global lexical and class
  declarations, for-of/for-in), including break/continue completions that
  carry a value out of such a statement.
info: |
  ScriptEvaluation returns the completion of the Script body's StatementList
  (sec-runtime-semantics-scriptevaluation). StatementList evaluation uses
  UpdateEmpty to keep the last non-empty value (sec-block-runtime-semantics-evaluation);
  loops, switch, try and labelled statements apply UpdateEmpty to their own
  completion (sec-updateempty).
esid: sec-runtime-semantics-scriptevaluation
features: [Symbol.iterator]
---*/

function script(src) {
  return $262.evalScript(src);
}

assert.sameValue(script("1; try { 2; } finally { 3; }"), 2, "try/finally keeps the try block value");
assert.sameValue(script("1; try { 2; } catch (e) { 3; }"), 2, "try/catch, no throw");
assert.sameValue(script("1; try { throw 0; } catch (e) { 4; }"), 4, "catch value");
assert.sameValue(script("1; try { } finally { 3; }"), undefined, "empty try is undefined, not the earlier 1");
assert.sameValue(script("1; switch (1) { case 1: 5; break; }"), 5, "switch with break value");
assert.sameValue(script("1; switch (1) {}"), undefined, "empty switch is undefined");
assert.sameValue(script("1; switch (2) { case 1: 5; }"), undefined, "unmatched switch is undefined");
assert.sameValue(script("1; class C1 {}"), 1, "class declaration is empty");
assert.sameValue(script("1; let l1 = 2;"), 1, "lexical declaration is empty");
assert.sameValue(script("const c2 = 2; c2;"), 2, "const read");
assert.sameValue(script("1; function f3() {} 2;"), 2, "function declaration is empty");
assert.sameValue(script("1; function f4() {}"), 1, "trailing function declaration is empty");
assert.sameValue(script("l5: { 7; break l5; }"), 7, "labelled block break value");
assert.sameValue(script("1; l6: { break l6; }"), undefined, "labelled block break without value");
assert.sameValue(script("1; l7: 2;"), 2, "labelled expression statement");
assert.sameValue(script("while (true) { 1; l8: { 8; break l8; } break; }"), 8, "labelled block inside a loop");
assert.sameValue(script("do { 4; try { 6; break; } finally {} } while (false);"), 6, "valued break out of try inside a loop");
assert.sameValue(script("for (var i9 = 0; i9 < 2; i9++) { 3; try { continue; } finally {} }"), undefined, "continue out of try resets the value");
assert.sameValue(script("for (var i10 = 0; i10 < 3; i10++) { i10; switch (i10) { case 1: continue; } }"), undefined, "continue out of switch");
assert.sameValue(script("x11: for (var i11 = 0; i11 < 3; i11++) { 5; for (var j11 of [1]) { 9; break x11; } }"), 9, "labelled break from a nested for-of");
assert.sameValue(script("x12: for (var i12 = 0; i12 < 3; i12++) { 5; for (var j12 of [1]) { 9; continue x12; } }"), 9, "labelled continue from a nested for-of");
assert.sameValue(script("var o13 = { a: 1 }; for (var k13 in o13) { k13; }"), "a", "for-in value");
assert.sameValue(script("var n14 = 0; for (let i = 0; i < 3; i++) { n14 += i; i; }"), 2, "for with a lexical head");
assert.sameValue(script("{ let a15 = 1; a15; }"), 1, "block with a lexical binding");
assert.sameValue(script("1; { let a16 = 1; }"), 1, "block with only a declaration is empty");
assert.sameValue(script("3; do { try { 5; continue; } finally {} } while (false);"), 5, "continue value out of try inside do-while");

var kept = script("var collect = $262.gc; try { ({ marker: 1 }); } finally {} var ignored17 = collect();");
assert.sameValue(kept.marker, 1, "a completion object from a tree-walker unit survives a later collection");
