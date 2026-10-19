/*---
description: >
  The callee, receiver and every argument of a call or `new` with spread
  arguments stay reachable while later spreads, argument expressions and the
  callee itself trigger garbage collection, and nothing is left rooted when the
  evaluation throws.
esid: sec-argumentlistevaluation
features: [Symbol.iterator]
flags: [noStrict]
info: |
  ArgumentListEvaluation keeps the values already appended to the list alive
  while the remaining arguments are evaluated, and EvaluateCall holds the
  function and this value across the whole evaluation.
---*/

function fresh(marker) {
  return { inner: { marker: marker } };
}

function gcThen(value) {
  $262.gc();
  return value;
}

function collect() {
  return Array.prototype.slice.call(arguments);
}

function iteratorOf(values, collectOnEnd) {
  var i = 0;
  return {
    [Symbol.iterator]() { return this; },
    next() {
      if (i < values.length) return { value: values[i++], done: false };
      if (collectOnEnd) $262.gc();
      return { done: true };
    }
  };
}

function check(list, markers, label) {
  assert.sameValue(list.length, markers.length, label + ": length");
  for (var i = 0; i < markers.length; i++) {
    assert.sameValue(list[i].inner.marker, markers[i], label + ": argument " + i);
  }
}

(function () {
  var r = collect(fresh("a"), ...iteratorOf([fresh("b"), fresh("c")], true), gcThen(fresh("d")), ...[gcThen(fresh("e"))], fresh("f"));
  check(r, ["a", "b", "c", "d", "e", "f"], "mixed");
})();

(function () {
  var r = collect(...[fresh("x")], ...iteratorOf([fresh("y")], true), ...gcThen([fresh("z")]));
  check(r, ["x", "y", "z"], "spreads only");
})();

(function () {
  var o = { m: function () { $262.gc(); return [this.inner.marker, Array.prototype.slice.call(arguments)]; }, inner: { marker: "recv" } };
  function receiver() { return { m: o.m, inner: { marker: "fresh receiver" } }; }
  var out = receiver().m(...iteratorOf([fresh("p"), fresh("q")], true), gcThen(fresh("r")));
  assert.sameValue(out[0], "fresh receiver", "receiver survives");
  check(out[1], ["p", "q", "r"], "method call");
})();

(function () {
  function makeCallee() {
    var marker = { v: "callee" };
    return function () { $262.gc(); return [marker.v, arguments.length]; };
  }
  var out = makeCallee()(...iteratorOf([fresh("1"), fresh("2")], true), gcThen(fresh("3")));
  assert.sameValue(out[0], "callee", "callee survives");
  assert.sameValue(out[1], 3, "argument count");
})();

(function () {
  function C() {
    $262.gc();
    this.list = Array.prototype.slice.call(arguments);
  }
  var c = new C(fresh("n1"), ...iteratorOf([fresh("n2"), fresh("n3")], true), gcThen(fresh("n4")));
  check(c.list, ["n1", "n2", "n3", "n4"], "new");
})();

(function () {
  var big = [];
  for (var i = 0; i < 600; i++) big.push(fresh("b" + i));
  function count() { $262.gc(); return arguments; }
  var args = count(...big, ...iteratorOf([fresh("tail")], true));
  assert.sameValue(args.length, 601, "large spread length");
  assert.sameValue(args[0].inner.marker, "b0", "first");
  assert.sameValue(args[599].inner.marker, "b599", "last of the array");
  assert.sameValue(args[600].inner.marker, "tail", "iterator tail");
})();

(function () {
  for (var round = 0; round < 3; round++) {
    var caught = 0;
    try {
      collect(fresh("t"), ...iteratorOf([fresh("u")]), (function () { $262.gc(); throw new Test262Error("boom"); })(), ...[fresh("never")]);
    } catch (e) {
      caught++;
    }
    assert.sameValue(caught, 1, "argument throw is caught");
    caught = 0;
    try {
      collect(fresh("t"), ...{ [Symbol.iterator]() { $262.gc(); throw new Test262Error("iter"); } });
    } catch (e) {
      caught++;
    }
    assert.sameValue(caught, 1, "iterator throw is caught");
    caught = 0;
    try {
      (function () { $262.gc(); throw new Test262Error("callee"); })(...[fresh("v")], fresh("w"));
    } catch (e) {
      caught++;
    }
    assert.sameValue(caught, 1, "callee throw is caught");
  }
  $262.gc();
  assert.sameValue(collect(...[fresh("after")])[0].inner.marker, "after", "state is balanced after throws");
})();

(function () {
  function* gen() {
    var got = collect(fresh("g1"), ...(yield), gcThen(fresh("g2")));
    return got;
  }
  var g = gen();
  g.next();
  $262.gc();
  var done = g.next(iteratorOf([fresh("g3")], true));
  check(done.value, ["g1", "g3", "g2"], "generator");
})();
