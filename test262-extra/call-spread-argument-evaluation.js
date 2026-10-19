/*---
description: >
  Spread arguments of calls and `new` evaluate the callee, the arguments and
  each spread's iterator protocol strictly in order, build the argument list
  before the call, and report abrupt completions where ArgumentListEvaluation
  does.
esid: sec-argumentlistevaluation
features: [Symbol.iterator, generators, Reflect]
includes: [compareArray.js]
flags: [noStrict]
info: |
  ArgumentListEvaluation of `...AssignmentExpression` evaluates the expression,
  calls GetIterator on its value, then repeats IteratorStepValue, appending each
  value, before the next argument is evaluated. The call (or the IsConstructor
  check of `new`) happens after the whole list is built.
---*/

var log = [];

function iterable(name, values, opts) {
  opts = opts || {};
  return {
    get [Symbol.iterator]() {
      log.push(name + ":@@iterator");
      return function () {
        log.push(name + ":call@@iterator");
        var i = 0;
        return {
          get next() {
            log.push(name + ":get next");
            return function () {
              log.push(name + ":next");
              if (opts.throwAt === i) throw new Test262Error(name + " next");
              if (i >= values.length) return { get done() { log.push(name + ":done"); return true; }, value: undefined };
              var v = values[i++];
              return {
                get done() { log.push(name + ":done"); return false; },
                get value() { log.push(name + ":value"); return v; }
              };
            };
          },
          return: function () {
            log.push(name + ":return");
            return {};
          }
        };
      };
    }
  };
}

function tag(name, v) {
  log.push(name);
  return v;
}

function collect() {
  return Array.prototype.slice.call(arguments);
}

// Callee, `this`, plain arguments and spreads interleave in source order.
(function () {
  log = [];
  var o = { m: collect };
  var r = (tag("base", o), o).m(tag("a", 1), ...iterable("s1", [2, 3]), tag("b", 4), ...iterable("s2", [5]), tag("c", 6));
  assert.compareArray(r, [1, 2, 3, 4, 5, 6], "values in order");
  assert.compareArray(log, [
    "base",
    "a",
    "s1:@@iterator", "s1:call@@iterator", "s1:get next",
    "s1:next", "s1:done", "s1:value",
    "s1:next", "s1:done", "s1:value",
    "s1:next", "s1:done",
    "b",
    "s2:@@iterator", "s2:call@@iterator", "s2:get next",
    "s2:next", "s2:done", "s2:value",
    "s2:next", "s2:done",
    "c"
  ], "evaluation and iteration order");
})();

// The callee is evaluated before the arguments; a non-callable throws after them.
(function () {
  log = [];
  var notCallable = {};
  assert.throws(TypeError, function () {
    tag("callee", notCallable)(...iterable("s", [1]), tag("after", 2));
  });
  assert.compareArray(log.filter(function (e) { return e === "callee" || e === "after" || e === "s:next"; }),
    ["callee", "s:next", "s:next", "after"], "arguments are evaluated before the TypeError");

  log = [];
  assert.throws(TypeError, function () {
    new (tag("ctor", function () {}.bind().call.bind({}))) (...iterable("s", [1]), tag("after", 2));
  });
  assert.sameValue(log.indexOf("after") >= 0, true, "new evaluates arguments before IsConstructor");

  log = [];
  assert.throws(TypeError, function () {
    new (tag("arrow", () => {}))(...iterable("s", []), tag("after", 2));
  });
  assert.compareArray(log.filter(function (e) { return e === "arrow" || e === "after"; }), ["arrow", "after"], "arrow: IsConstructor after the arguments");
})();

// Errors from the iterator protocol propagate; a throwing `next` does not close.
(function () {
  log = [];
  assert.throws(Test262Error, function () {
    collect(tag("first", 1), ...iterable("s", [1, 2, 3], { throwAt: 1 }), tag("never", 0));
  });
  assert.sameValue(log.indexOf("never"), -1, "later arguments are not evaluated");
  assert.sameValue(log.indexOf("s:return"), -1, "a throwing next does not close the iterator");

  assert.throws(TypeError, function () { collect(...undefined); }, "undefined is not iterable");
  assert.throws(TypeError, function () { collect(1, ...null, 2); }, "null is not iterable");
  assert.throws(TypeError, function () { collect(...5); }, "a number is not iterable");
  assert.throws(TypeError, function () { collect(...{}); }, "an object without @@iterator");
  assert.throws(TypeError, function () { new collect(...{}); }, "new: not iterable");
  var badNext = { [Symbol.iterator]() { return { next() { return 1; } }; } };
  assert.throws(TypeError, function () { collect(...badNext); }, "next returns a non-object");
})();

// Strings, Sets, Maps, generators and arguments objects iterate through @@iterator.
(function () {
  assert.compareArray(collect(...""), [], "empty string");
  assert.compareArray(collect(..."a\u{1F600}b"), ["a", "\u{1F600}", "b"], "string code points");
  assert.compareArray(collect(...new Set([1, 2, 2, 3])), [1, 2, 3], "Set");
  var entries = collect(...new Map([[1, "a"]]));
  assert.sameValue(entries.length, 1, "Map entry count");
  assert.compareArray(entries[0], [1, "a"], "Map entry");
  function* g() { yield 1; yield 2; return 3; }
  assert.compareArray(collect(0, ...g(), 9), [0, 1, 2, 9], "generator");
  function viaArguments() { return collect(...arguments, "end"); }
  assert.compareArray(viaArguments(1, 2), [1, 2, "end"], "arguments object");
  assert.compareArray(collect(...[...[1, 2], 3]), [1, 2, 3], "nested array spread");
})();

// Holes read as undefined; element getters run once, in order, before the call.
(function () {
  var holey = [1, , 3];
  var r = collect(...holey);
  assert.sameValue(r.length, 3, "length");
  assert.sameValue(1 in r, true, "a hole becomes an own undefined argument");
  assert.sameValue(r[1], undefined, "value");

  log = [];
  var withGetter = [0, 0];
  Object.defineProperty(withGetter, 0, { get: function () { log.push("g0"); return "x"; } });
  Object.defineProperty(withGetter, 1, { get: function () { log.push("g1"); return "y"; } });
  var r2 = collect(...withGetter, tag("tail", 1));
  assert.compareArray(r2, ["x", "y", 1], "getter values");
  assert.compareArray(log, ["g0", "g1", "tail"], "getters before later arguments");

  Array.prototype[1] = "inherited";
  try {
    assert.compareArray(collect(...[0, , 2]), [0, "inherited", 2], "a hole reads the prototype");
  } finally {
    delete Array.prototype[1];
  }
})();

// The list is a snapshot: the callee may mutate the spread array.
(function () {
  var arr = [1, 2, 3];
  function mutate() {
    arr.length = 0;
    arr.push(9);
    return Array.prototype.slice.call(arguments);
  }
  assert.compareArray(mutate(...arr), [1, 2, 3], "arguments are a snapshot");
  var seen;
  var iter = [1, 2, 3];
  iter[Symbol.iterator] = function () {
    var i = 0;
    return { next: function () { return i < 3 ? { value: i++, done: false } : { done: true }; } };
  };
  assert.compareArray(collect(...iter), [0, 1, 2], "own @@iterator on an array is honoured");
})();

// The Array iteration fast path honours patched built-ins.
(function () {
  var proto = Object.getPrototypeOf([][Symbol.iterator]());
  var originalNext = proto.next;
  var originalIterator = Array.prototype[Symbol.iterator];
  var calls = 0;
  proto.next = function () {
    calls++;
    return originalNext.call(this);
  };
  try {
    assert.compareArray(collect(...[1, 2]), [1, 2], "patched next: values");
    assert.sameValue(calls, 3, "patched next is called for each step and the end");
  } finally {
    proto.next = originalNext;
  }
  Array.prototype[Symbol.iterator] = function* () { yield "patched"; };
  try {
    assert.compareArray(collect(...[1, 2]), ["patched"], "patched Array.prototype[@@iterator]");
  } finally {
    Array.prototype[Symbol.iterator] = originalIterator;
  }
})();

// Removing and re-adding the built-ins moves their property slots; the fast path never trusts a stale one.
(function () {
  var arrayProto = Array.prototype;
  var iterProto = Object.getPrototypeOf([][Symbol.iterator]());
  var iteratorDesc = Object.getOwnPropertyDescriptor(arrayProto, Symbol.iterator);
  var nextDesc = Object.getOwnPropertyDescriptor(iterProto, "next");
  assert.compareArray(collect(...[1, 2]), [1, 2], "trains the lookups");

  delete arrayProto[Symbol.iterator];
  try {
    assert.throws(TypeError, function () { collect(...[1]); }, "no @@iterator");
    assert.throws(TypeError, function () { collect(...[]); }, "no @@iterator, empty");
  } finally {
    Object.defineProperty(arrayProto, Symbol.iterator, iteratorDesc);
  }
  assert.compareArray(collect(...[1, 2]), [1, 2], "@@iterator re-added");

  delete iterProto.next;
  try {
    assert.throws(TypeError, function () { collect(...[1]); }, "no next");
  } finally {
    Object.defineProperty(iterProto, "next", nextDesc);
  }
  assert.compareArray(collect(...[3, 4]), [3, 4], "next re-added");

  for (var i = 0; i < 40; i++) arrayProto["extra" + i] = i;
  try {
    delete arrayProto[Symbol.iterator];
    Object.defineProperty(arrayProto, Symbol.iterator, {
      value: function* () { yield "replaced"; },
      writable: true,
      configurable: true
    });
    assert.compareArray(collect(...[1, 2]), ["replaced"], "a re-added replacement is honoured");
  } finally {
    for (var j = 0; j < 40; j++) delete arrayProto["extra" + j];
    delete arrayProto[Symbol.iterator];
    Object.defineProperty(arrayProto, Symbol.iterator, iteratorDesc);
  }
  assert.compareArray(collect(...[5, 6]), [5, 6], "original restored");
})();

// Receivers: member, computed member and plain calls.
(function () {
  var o = { label: "o", m: function () { return [this.label].concat(Array.prototype.slice.call(arguments)); } };
  assert.compareArray(o.m(...[1, 2]), ["o", 1, 2], "o.m(...)");
  assert.compareArray(o["m"](0, ...[1]), ["o", 0, 1], "o['m'](...)");
  var key = { toString: function () { log.push("key"); return "m"; } };
  log = [];
  assert.compareArray(o[key](...iterable("s", [1])), ["o", 1], "computed key");
  assert.sameValue(log[0], "key", "the key is converted before the arguments");
  function sloppyThis() { return this; }
  assert.sameValue(sloppyThis(...[]), this, "sloppy plain call");
  assert.sameValue((0, o.m)(...[])[0], undefined, "(0, o.m) loses the receiver");
  var obj = { f() { return () => [this.label, arguments.length]; }, label: "arrow" };
  assert.compareArray(obj.f(...[1, 2, 3])(...[7]), ["arrow", 3], "arrow keeps this and the enclosing arguments");
  assert.sameValue(typeof Function.prototype.call.call(function () { return this; }, 1, ...[]), "object", "call(thisArg, ...) boxes in sloppy callee");
})();

// A `with` scope supplies the receiver of a bare name.
(function () {
  var scope = { m: function () { return this === scope; } };
  with (scope) {
    assert.sameValue(m(...[]), true, "with base object is the receiver");
  }
})();

// `new` with spread arguments.
(function () {
  function C(a, b, c) {
    this.args = Array.prototype.slice.call(arguments);
    this.nt = new.target === C;
  }
  var c = new C(1, ...[2, 3], 4);
  assert.compareArray(c.args, [1, 2, 3, 4], "new C(...)");
  assert.sameValue(c.nt, true, "new.target");
  assert.sameValue(c instanceof C, true, "instance");
  assert.sameValue(new C(...[]).args.length, 0, "empty spread");
  class D { constructor(...args) { this.n = args.length; } }
  assert.sameValue(new D(...new Array(300).fill(0)).n, 300, "class constructor, 300 arguments");
  assert.sameValue(new (class { constructor(a) { this.a = a; } })(...[5]).a, 5, "anonymous class");
  assert.sameValue(new Date(...[2020, 0, 2]).getDate(), 2, "native constructor");
  assert.sameValue(new Array(...[3]).length, 3, "Array(...)");
  var bound = C.bind(null, 1);
  assert.compareArray(new bound(...[2]).args, [1, 2], "bound constructor");
  assert.throws(TypeError, function () { new Math.max(...[1]); }, "not a constructor");
  assert.throws(TypeError, function () { new (async function () {})(...[1]); }, "async function");
  var Made = new Function("this.x = arguments.length");
  assert.sameValue(new Made(...[1, 2]).x, 2, "Function-constructor function");
  assert.sameValue(Reflect.construct(C, [1]).args.length, 1, "control");
})();

// Sizes around the dense fast path and well past it.
(function () {
  [0, 1, 2, 3, 12, 13, 255, 256, 257, 1000, 5000].forEach(function (n) {
    var src = [];
    for (var i = 0; i < n; i++) src.push(i);
    var r = collect(...src);
    assert.sameValue(r.length, n, "length " + n);
    assert.sameValue(r[n - 1], n ? n - 1 : undefined, "last " + n);
    var r2 = collect("a", ...src, "z");
    assert.sameValue(r2.length, n + 2, "wrapped length " + n);
    assert.sameValue(r2[n + 1], "z", "trailing argument " + n);
  });
  var big = new Array(2000).fill(1);
  assert.sameValue(Math.max(...big, 5), 5, "Math.max");
  assert.sameValue(String.fromCharCode(...[72, 105]), "Hi", "native callee");
  var target = [];
  target.push(...[1, 2], ...[3]);
  assert.compareArray(target, [1, 2, 3], "push(...)");
})();

// Several spreads and arguments after them keep the count right.
(function () {
  assert.compareArray(collect(...[1], ...[], ...[2, 3]), [1, 2, 3], "adjacent spreads");
  assert.compareArray(collect(...[], ...[]), [], "empty spreads");
  assert.compareArray(collect(...[1], 2, 3, ...[4], 5, ...[6, 7], 8, 9), [1, 2, 3, 4, 5, 6, 7, 8, 9], "mixed");
  assert.compareArray(collect(0, 1, ...[], 2), [0, 1, 2], "plain before and after an empty spread");
  function inner() { return [...arguments]; }
  assert.compareArray(collect(...inner(...[1, 2], 3), ...inner(4)), [1, 2, 3, 4], "nested calls");
  assert.compareArray(collect(...collect(...collect(...[1, 2]))), [1, 2], "deeply nested");
})();

// Abrupt completions and handlers around a spread call.
(function () {
  var r = [];
  try {
    collect(1, 2, ...[3, 4], (function () { throw new Test262Error("arg"); })(), 5);
  } catch (e) {
    r.push(e.message);
  } finally {
    r.push("finally");
  }
  assert.compareArray(r, ["arg", "finally"], "throw in a later argument");
  function thrower() { throw new Test262Error("callee"); }
  var caught;
  try {
    thrower(...[1, 2, 3]);
  } catch (e) {
    caught = e;
  }
  assert.sameValue(caught instanceof Test262Error, true, "throw from the callee");
  var out = [];
  for (var i = 0; i < 3; i++) {
    try {
      out.push(collect(...[i, i], ...(i === 1 ? undefined : [])).length);
    } catch (e) {
      out.push("T");
    }
  }
  assert.compareArray(out, [2, "T", 2], "handler resumes with a clean operand stack");
  label: {
    collect(...[1], 2);
    break label;
  }
  var viaLoop = 0;
  for (var j = 0; j < 4; j++) {
    viaLoop += collect(...[j], j).length;
    if (j == 2) continue;
  }
  assert.sameValue(viaLoop, 8, "loops around spread calls");
})();

// Generators: `yield` inside spread operands and arguments.
(function () {
  function* gen() {
    var r = collect("x", ...(yield 1), (yield 2), ...(yield 3));
    return r;
  }
  var g = gen();
  assert.sameValue(g.next().value, 1);
  assert.sameValue(g.next([10, 11]).value, 2);
  assert.sameValue(g.next("two").value, 3);
  var done = g.next([30]);
  assert.compareArray(done.value, ["x", 10, 11, "two", 30], "values across suspensions");
  assert.sameValue(done.done, true);

  var h = gen();
  h.next();
  assert.sameValue(h.return(7).value, 7, "return() abandons the pending call");
  var t = gen();
  t.next();
  assert.throws(Test262Error, function () { t.throw(new Test262Error("t")); }, "throw() propagates");
  assert.sameValue(t.next().done, true, "generator is finished");
})();

// Constructs that stay on the tree-walker still work next to native spread calls.
(function () {
  class A { constructor(...args) { this.args = args; } m(...args) { return args; } }
  class B extends A {
    constructor(...args) { super(...args, "b"); }
    m(...args) { return super.m(...args, "bm"); }
  }
  var b = new B(1, 2);
  assert.compareArray(b.args, [1, 2, "b"], "super(...)");
  assert.compareArray(b.m(3), [3, "bm"], "super.m(...)");
  var o = { f: collect };
  assert.sameValue(o?.f(...[1, 2]).length, 2, "optional chain member call");
  assert.sameValue(o.g?.(...[1, 2]), undefined, "optional call short-circuits");
  assert.sameValue(o.f?.(...[1]).length, 1, "optional call");
  var x = "outer";
  function direct() { var x = "inner"; return eval(...["x"]); }
  assert.sameValue(direct(), "inner", "direct eval with spread arguments");
  var indirect = eval;
  assert.sameValue(indirect(...["typeof x"]), "undefined", "indirect eval with spread arguments");
})();

// A strict tail call keeps its constant stack.
(function () {
  "use strict";
  function loop(n, ...rest) {
    if (n === 0) return "done";
    return loop(...[n - 1], ...rest);
  }
  assert.sameValue(loop(10), "done", "spread tail call");
})();
