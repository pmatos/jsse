"use strict";
const assert = require("assert");

function differ(actual, expected) {
    const e = new Error("values differ");
    e.actual = actual;
    e.expected = expected;
    e.showDiff = true;
    throw e;
}

describe("diff", function () {
    it("numbers", () => differ(1, 2));
    it("null and undefined", () => differ(null, undefined));
    it("objects", () => differ({ a: 1, b: { c: [1, 2] }, d: "x" }, { a: 1, b: { c: [1, 3] }, d: "y" }));
    it("key order is canonical", () => differ({ b: 1, a: 2 }, { a: 2, b: 1, c: 3 }));
    it("arrays", () => differ([1, "two", [3], { four: 4 }], [1, "two", [3], { four: 5 }]));
    it("multi-line strings", () => differ("one\ntwo\nthree\nfour", "one\n2\nthree\nfour\nfive"));
    it("a string and a number", () => differ("1", 1));
    it("maps, sets, dates and regexps", () =>
        differ(
            { m: new Map([[1, "a"]]), s: new Set([1, 2]), d: new Date(0), r: /x/g },
            { m: new Map([[1, "b"]]), s: new Set([1, 3]), d: new Date(1000), r: /x/i }
        ));
    it("special numbers", () => differ([NaN, -0, Infinity, 1n], [NaN, 0, -Infinity, 2n]));
    it("functions and symbols", () =>
        differ({ f() {}, s: Symbol("s"), u: undefined }, { f() {}, s: Symbol("t") }));
    it("a circular structure", () => {
        const a = { name: "a" };
        a.self = a;
        const b = { name: "b" };
        b.self = b;
        differ(a, b);
    });
    it("typed arrays and buffers", () =>
        differ({ u: new Uint8Array([1, 2]), b: Buffer.from("ab") }, { u: new Uint8Array([1, 3]), b: Buffer.from("ac") }));
    it("class instances", () => {
        class Point { constructor(x) { this.x = x; } }
        differ(new Point(1), new Point(2));
    });
    it("a long list", () =>
        differ(Array.from({ length: 60 }, (_, i) => i), Array.from({ length: 60 }, (_, i) => (i === 30 ? -1 : i))));
    it("showDiff false hides it", () => {
        const e = new Error("no diff");
        e.actual = 1;
        e.expected = 2;
        e.showDiff = false;
        throw e;
    });
    it("assert.strictEqual", () => assert.strictEqual("foo", "bar"));
    it("assert.strictEqual with a message", () => assert.strictEqual(1, 2, "numbers must agree"));
    it("assert.deepStrictEqual objects", () => assert.deepStrictEqual({ a: [1, { b: 2 }] }, { a: [1, { b: 3 }] }));
    it("assert.deepStrictEqual maps and sets", () =>
        assert.deepStrictEqual(new Map([[1, new Set([1])]]), new Map([[1, new Set([2])]])));
    it("assert.deepEqual", () => assert.deepEqual({ a: 1 }, { a: 2 }));
    it("assert.notStrictEqual", () => assert.notStrictEqual(1, 1));
    it("assert.equal", () => assert.equal(1, 2));
    it("assert.ok with a message", () => assert.ok(false, "it was false"));
    it("assert.match", () => assert.match("abc", /x/));
    it("assert.rejects", () => assert.rejects(Promise.resolve(1)));
    it("an AssertionError built by hand", () => {
        throw new assert.AssertionError({ actual: [1], expected: [2], operator: "deepStrictEqual" });
    });
    it("passes", () => {});
});
