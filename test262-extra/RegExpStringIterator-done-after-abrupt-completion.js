// Copyright (C) 2026 the JSSE authors. All rights reserved.
/*---
esid: sec-%regexpstringiteratorprototype%.next
description: >
  A RegExp String Iterator is finished after next() completes abruptly: a
  throwing exec, a throwing lastIndex read or write, or a throwing ToString of
  the match, leaves later next() calls returning { value: undefined, done: true }.
info: |
  22.2.9.2.1 %RegExpStringIteratorPrototype%.next ( )
    1. Return ? GeneratorResume(this value, empty, "%RegExpStringIteratorPrototype%").

  22.2.9.1 CreateRegExpStringIterator ( R, S, global, fullUnicode )
    The closure is a generator body: an abrupt completion of any step
    (RegExpExec, ToString(Get(match, "0")), ToLength(Get(R, "lastIndex")),
    Set(R, "lastIndex", ...)) terminates the generator.
features: [Symbol.matchAll, String.prototype.matchAll]
---*/

function assertDone(it, label) {
  var r = it.next();
  assert.sameValue(r.done, true, label + " done");
  assert.sameValue(r.value, undefined, label + " value");
}

// RegExp.prototype[@@matchAll] builds the iterating object with the species
// constructor, which may return any object with exec and lastIndex.
function makeRegExp(flags, hooks) {
  var execCalls = 0;
  var value = 0;
  var matcher = {
    exec: function (s) {
      execCalls++;
      return hooks.exec.call(this, s, execCalls);
    }
  };
  Object.defineProperty(matcher, "lastIndex", {
    get: function () { return hooks.lastIndex ? hooks.lastIndex.get(value) : value; },
    set: function (v) {
      if (hooks.lastIndex) hooks.lastIndex.set(v);
      value = v;
    },
    configurable: true
  });
  var re = new RegExp("a", flags);
  re.constructor = {};
  re.constructor[Symbol.species] = function () { return matcher; };
  return {
    re: re,
    calls: function () { return execCalls; }
  };
}

// exec throws (global and non-global)
[true, false].forEach(function (g) {
  var h = makeRegExp(g ? "g" : "", {
    exec: function () { throw new Test262Error("exec"); }
  });
  var it = h.re[Symbol.matchAll]("aaa");
  assert.throws(Test262Error, function () { it.next(); }, "exec throws " + g);
  assertDone(it, "after exec throw " + g);
  assertDone(it, "after exec throw again " + g);
  assert.sameValue(h.calls(), 1, "exec not called again " + g);
});

// lastIndex read throws after an empty match
(function () {
  var armed = false;
  var h = makeRegExp("g", {
    exec: function () { return [""]; },
    lastIndex: {
      get: function (v) { if (armed) throw new Test262Error("get"); return v; },
      set: function () {}
    }
  });
  var it = h.re[Symbol.matchAll]("abc");
  armed = true;
  assert.throws(Test262Error, function () { it.next(); }, "lastIndex get throws");
  assertDone(it, "after lastIndex get throw");
  assert.sameValue(h.calls(), 1, "exec not called again");
})();

// lastIndex write throws after an empty match
(function () {
  var armed = false;
  var h = makeRegExp("g", {
    exec: function () { return [""]; },
    lastIndex: {
      get: function (v) { return v; },
      set: function () { if (armed) throw new Test262Error("set"); }
    }
  });
  var it = h.re[Symbol.matchAll]("abc");
  armed = true;
  assert.throws(Test262Error, function () { it.next(); }, "lastIndex set throws");
  assertDone(it, "after lastIndex set throw");
  assert.sameValue(h.calls(), 1, "exec not called again");
})();

// lastIndex valueOf throws (ToLength)
(function () {
  var h = makeRegExp("g", {
    exec: function () { return [""]; },
    lastIndex: {
      get: function () {
        return { valueOf: function () { throw new Test262Error("valueOf"); } };
      },
      set: function () {}
    }
  });
  var it = h.re[Symbol.matchAll]("abc");
  assert.throws(Test262Error, function () { it.next(); }, "lastIndex valueOf throws");
  assertDone(it, "after lastIndex valueOf throw");
  assert.sameValue(h.calls(), 1, "exec not called again");
})();

// ToString of the match throws
(function () {
  var h = makeRegExp("g", {
    exec: function () {
      return { 0: { toString: function () { throw new Test262Error("toString"); } } };
    }
  });
  var it = h.re[Symbol.matchAll]("a");
  assert.throws(Test262Error, function () { it.next(); }, "match ToString throws");
  assertDone(it, "after match ToString throw");
  assert.sameValue(h.calls(), 1, "exec not called again");
})();

// A later call must not resume matching even though exec would now succeed
(function () {
  var shouldThrow = true;
  var h = makeRegExp("g", {
    exec: function (s) {
      if (shouldThrow) throw new Test262Error("first");
      return RegExp.prototype.exec.call(this, s);
    }
  });
  var it = h.re[Symbol.matchAll]("aa");
  assert.throws(Test262Error, function () { it.next(); }, "first call throws");
  shouldThrow = false;
  assertDone(it, "no resumption");
  assert.sameValue(h.calls(), 1, "exec not called again");
})();
