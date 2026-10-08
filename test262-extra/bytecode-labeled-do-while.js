/*---
description: >
  A labeled do-while loop runs its body before its test, and a matching
  continue evaluates the test before the next body execution.
info: |
  DoWhileLoopEvaluation starts V at undefined, evaluates the body, then
  evaluates the test after any matching continue. A matching break exits
  without evaluating the test. These cases also exercise jsse's bytecode
  lowering of labeled do-while loops (issue #54).
esid: sec-runtime-semantics-dowhileloopevaluation
---*/

function continueRunsTest() {
  var bodyCount = 0;
  var testCount = 0;
  loop: do {
    bodyCount++;
    if (bodyCount < 3) continue loop;
  } while (++testCount < 2);
  return bodyCount * 10 + testCount;
}
assert.sameValue(continueRunsTest(), 22, "continue evaluates the test");

function breakSkipsTest() {
  var testCount = 0;
  var bodyCount = 0;
  loop: do {
    bodyCount++;
    break loop;
  } while (++testCount < 3);
  return bodyCount * 10 + testCount;
}
assert.sameValue(breakSkipsTest(), 10, "break skips the test");

function bodyRunsBeforeFalseTest() {
  var count = 0;
  do {
    count++;
  } while (false);
  return count;
}
assert.sameValue(bodyRunsBeforeFalseTest(), 1, "body runs once before a false test");
