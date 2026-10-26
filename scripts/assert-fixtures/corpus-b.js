var zero = 0;
var obj = { fn: function () { return false; } };
add("ok.call", function () { assert(false); });
add("ok.method", function () { assert.ok(zero); });
add("ok.computed", function () { assert["ok"](zero); });
add("ok.optional", function () { assert?.ok(zero); });
add("ok.nested", function () { assert.ok(obj.fn(1, "a;b", `t${1}`, /x\)/)); });
add("ok.compare", function () { assert.ok(1 + 1 === 3, undefined); });
add("ok.statement", function () { var r = [1, 2].map(function (v) { return v; }); assert(r.length === 3); });
add("ok.twice", function () { assert.ok(true); assert.ok(zero); assert.ok(true); });
add("ok.multiline", function () {
  assert.ok(
    zero,
  );
});
add("ok.noargs", function () { assert.ok(); });
add("ok.message", function () { assert.ok(0, "custom"); });
add("ok.format", function () { assert.ok(0, "%s and %d", "str", 42); });
add("ok.fnmessage", function () { assert.ok(0, function (a, e) { return "fn:" + a + ":" + e; }); });
add("ok.errmessage", function () { assert.ok(0, new RangeError("range")); });
add("ok.badmessage", function () { assert.ok(0, 5); });
add("ok.ambiguous", function () { assert.ok(0, new Error("e"), "extra"); });
add("ok.fnmessage.nonstring", function () { assert.ok(0, function () { return 5; }); });
add("ok.fnmessage.throws", function () { assert.ok(0, function () { throw new Error("no"); }); });
add("ok.strict", function () { strict(zero); });

add("fail.none", function () { assert.fail(); });
add("fail.message", function () { assert.fail("boom"); });
add("fail.error", function () { assert.fail(new TypeError("te")); });
add("fail.object", function () { assert.fail({ a: 1 }); });

add("throws.missing", function () { assert.throws(function () {}); });
add("throws.missing.message", function () { assert.throws(function () {}, "custom"); });
add("throws.missing.class", function () { assert.throws(function () {}, TypeError, "custom"); });
add("throws.missing.name", function () { assert.throws(function () {}, TypeError); });
add("throws.regexp", function () { assert.throws(function () { throw new Error("abc"); }, /xyz/); });
add("throws.regexp.ok", function () { assert.throws(function () { throw new Error("abc"); }, /b/); });
add("throws.class", function () { assert.throws(function () { throw new Error("abc"); }, TypeError); });
add("throws.class.proto", function () {
  class A extends Error {}
  assert.throws(function () { throw new Error("abc"); }, A);
});
add("throws.object", function () { assert.throws(function () { throw new Error("abc"); }, { message: "abe", name: "Error" }); });
add("throws.object.regexp", function () { assert.throws(function () { throw new Error("abc"); }, { message: /z/ }); });
add("throws.object.extra", function () { var e = new Error("abc"); e.code = "X"; assert.throws(function () { throw e; }, { code: "Y", message: "abc" }); });
add("throws.function", function () { assert.throws(function () { throw new Error("abc"); }, function () { return false; }); });
add("throws.function.throws", function () { assert.throws(function () { throw new Error("abc"); }, function () { throw new Error("inner"); }); });
add("throws.error", function () { assert.throws(function () { throw new Error("abc"); }, new Error("abe")); });
add("throws.string", function () { assert.throws(function () { throw new Error("abc"); }, "abc"); });
add("throws.string.thrown", function () { assert.throws(function () { throw "abc"; }, "abc"); });
add("throws.invalid", function () { assert.throws(function () {}, 5); });
add("throws.notfn", function () { assert.throws(5); });
add("throws.primitive", function () { assert.throws(function () { throw 5; }, { a: 1 }); });
add("throws.null", function () { assert.throws(function () { throw null; }, /x/); });
add("throws.proto.null", function () { assert.throws(function () { throw Object.create(null); }, Error); });
add("throws.fn.name", function () { assert.throws(function () {}, function namedValidator() {}); });
add("doesNotThrow.thrown", function () { assert.doesNotThrow(function () { throw new Error("oops"); }); });
add("doesNotThrow.message", function () { assert.doesNotThrow(function () { throw new Error("oops"); }, "extra"); });
add("doesNotThrow.class", function () { assert.doesNotThrow(function () { throw new TypeError("oops"); }, TypeError, "extra"); });
add("doesNotThrow.otherclass", function () { assert.doesNotThrow(function () { throw new Error("oops"); }, TypeError); });
add("doesNotThrow.regexp", function () { assert.doesNotThrow(function () { throw new Error("oops"); }, /oops/); });
add("doesNotThrow.invalid", function () { assert.doesNotThrow(function () { throw new Error("oops"); }, 5); });
add("doesNotThrow.primitive", function () { assert.doesNotThrow(function () { throw 5; }); });

add("rejects.missing", function () { return assert.rejects(Promise.resolve(1)); });
add("rejects.missing.message", function () { return assert.rejects(async function () {}, "custom"); });
add("rejects.mismatch", function () { return assert.rejects(Promise.reject(new Error("a")), { message: "b" }); });
add("rejects.class", function () { return assert.rejects(Promise.reject(new Error("a")), TypeError); });
add("rejects.notpromise", function () { return assert.rejects(function () { return 1; }); });
add("rejects.badarg", function () { return assert.rejects(5); });
add("rejects.ok", function () { return assert.rejects(Promise.reject(new Error("a")), /a/); });
add("doesNotReject.rejected", function () { return assert.doesNotReject(Promise.reject(new Error("a"))); });
add("doesNotReject.message", function () { return assert.doesNotReject(async function () { throw new Error("a"); }, "extra"); });
add("doesNotReject.notpromise", function () { return assert.doesNotReject(function () { return 1; }); });

add("match.nomatch", function () { assert.match("abc", /x/); });
add("match.nonstring", function () { assert.match(5, /x/); });
add("match.badregexp", function () { assert.match("a", "a"); });
add("match.message", function () { assert.match("abc", /x/, "custom"); });
add("match.multiline", function () { assert.match("a\nb\nc", /z/m); });
add("doesNotMatch.match", function () { assert.doesNotMatch("abc", /b/); });
add("doesNotMatch.nonstring", function () { assert.doesNotMatch(5, /x/); });

add("ifError.error", function () { assert.ifError(new Error("boom")); });
add("ifError.emptymsg", function () { assert.ifError(new TypeError()); });
add("ifError.string", function () { assert.ifError("str"); });
add("ifError.object", function () { assert.ifError({ a: 1 }); });
add("ifError.zero", function () { assert.ifError(0); });
add("ifError.null", function () { assert.ifError(null); });

add("missing.args", function () { assert.strictEqual(1); });
add("missing.args.deep", function () { assert.deepEqual(); });
add("custom.strictEqual", function () { assert.strictEqual(1, 2, "mine"); });
add("custom.strictEqual.format", function () { assert.strictEqual(1, 2, "%d vs %s", 1, "x"); });
add("custom.deepStrictEqual", function () { assert.deepStrictEqual({ a: 1 }, { a: 2 }, "mine"); });
add("custom.fn", function () { assert.strictEqual(1, 2, function (a, e) { return "got " + a + " want " + e; }); });
add("custom.error", function () { assert.equal(1, 2, new SyntaxError("se")); });

add("Assert.new", function () { return new assert.Assert() instanceof assert.Assert; });
add("Assert.call", function () { assert.Assert(); });
add("Assert.baddiff", function () { new assert.Assert({ diff: "bad" }); });
add("Assert.full", function () { new assert.Assert({ diff: "full" }).deepStrictEqual(big, big2); });
add("Assert.simple", function () { new assert.Assert({ diff: "simple" }).deepStrictEqual(big, big2); });
add("Assert.nonstrict", function () { new assert.Assert({ strict: false }).equal(1, 2); });
add("Assert.nonstrict.ok", function () { return new assert.Assert({ strict: false }).equal(1, "1"); });
add("Assert.skipPrototype", function () { return new assert.Assert({ skipPrototype: true }).deepStrictEqual(new Foo(), new Bar()); });
add("AssertionError.ctor", function () { throw new assert.AssertionError({ actual: 1, expected: 2, operator: "strictEqual" }); });
add("AssertionError.message", function () { throw new assert.AssertionError({ message: "m" }); });
add("AssertionError.noargs", function () { new assert.AssertionError(); });
add("AssertionError.operator", function () { throw new assert.AssertionError({ actual: 1, expected: 2, operator: "custom" }); });
add("AssertionError.props", function () {
  var e = new assert.AssertionError({ actual: [1], expected: [2], operator: "deepEqual" });
  return [e.name, e.code, e.generatedMessage, e.operator, Object.keys(e).join(), String(e).split("\n")[0], e instanceof Error];
});
add("AssertionError.inspect", function () {
  var e = new assert.AssertionError({ message: "m", actual: "a".repeat(600), expected: "b", operator: "strictEqual" });
  return util.inspect(e).split("\n").filter(function (l) { return !/^\s+at /.test(l); }).join("\n");
});
add("exports", function () { return Object.keys(assert).join() + "|" + Object.keys(strict).join() + "|" + (assert.strict === strict) + "|" + typeof assert.CallTracker; });
