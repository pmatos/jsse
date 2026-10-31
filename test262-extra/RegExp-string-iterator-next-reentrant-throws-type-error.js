// Copyright (C) 2026 the JSSE authors. All rights reserved.
/*---
esid: sec-%regexpstringiteratorprototype%.next
description: >
  %RegExpStringIteratorPrototype%.next throws a TypeError when called while the
  iterator's closure is already executing, and the iterator then completes
info: |
  22.2.9.2.1 %RegExpStringIteratorPrototype%.next ( )

    1. Return ? GeneratorResume(this value, empty, "%RegExpStringIteratorPrototype%").

  27.5.3.3 GeneratorResume ( generator, value, brand )

    1. Let state be ? GeneratorValidate(generator, brand).

  27.5.3.2 GeneratorValidate ( generator, brand )

    5. If state is executing, throw a TypeError exception.

  The closure calls RegExpExec on the matcher, which observes an "exec"
  override installed through the species constructor.
features: [Symbol.matchAll, Symbol.species, class]
---*/

var iter;
var nested;
var mode;

class Reentrant extends RegExp {
  exec(string) {
    if (mode === "reenter") {
      mode = "inner";
      try {
        iter.next();
        nested = "no throw";
      } catch (e) {
        nested = e.constructor;
      }
    } else if (mode === "throw") {
      mode = "inner";
      try {
        iter.next();
        nested = "no throw";
      } catch (e) {
        nested = e.constructor;
      }
      throw new Test262Error("exec failure");
    }
    return super.exec(string);
  }
}

function start(flags, subject, newMode) {
  mode = newMode;
  nested = undefined;
  iter = new Reentrant("a", flags)[Symbol.matchAll](subject);
}

start("g", "aa", "reenter");
var first = iter.next();
assert.sameValue(nested, TypeError, "global: nested next throws TypeError");
assert.sameValue(first.done, false, "global: outer next still yields a match");
assert.sameValue(first.value[0], "a", "global: outer match value");
assert.sameValue(iter.next().done, false, "global: second match");
assert.sameValue(iter.next().done, true, "global: exhausted");

start("", "aa", "reenter");
var firstNonGlobal = iter.next();
assert.sameValue(nested, TypeError, "non-global: nested next throws TypeError");
assert.sameValue(firstNonGlobal.done, false, "non-global: outer next yields the match");
assert.sameValue(iter.next().done, true, "non-global: completes after one match");

start("g", "b", "reenter");
var noMatch = iter.next();
assert.sameValue(nested, TypeError, "no match: nested next throws TypeError");
assert.sameValue(noMatch.done, true, "no match: outer next completes");
assert.sameValue(noMatch.value, undefined, "no match: outer value is undefined");

start("g", "aa", "throw");
assert.throws(Test262Error, function () {
  iter.next();
}, "exec abrupt completion propagates");
assert.sameValue(nested, TypeError, "nested next throws before the abrupt completion");
var afterThrow = iter.next();
assert.sameValue(afterThrow.done, true, "iterator is completed after an abrupt completion");
assert.sameValue(afterThrow.value, undefined, "completed iterator value is undefined");

mode = "none";
iter = new Reentrant("a", "g")[Symbol.matchAll]("aa");
assert.sameValue(iter.next().done, false, "no re-entrance: first match");
assert.sameValue(iter.next().done, false, "no re-entrance: second match");
assert.sameValue(iter.next().done, true, "no re-entrance: exhausted");
