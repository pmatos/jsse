"use strict";
const assert = require("assert");

describe("fail", function () {
    it("strictEqual", () => assert.strictEqual(1, 2));
    it("deepStrictEqual diff", () => assert.deepStrictEqual({ a: 1, b: [1, 2, 3], c: "x" }, { a: 1, b: [1, 2, 4], c: "y" }));
    it("string diff", () => assert.strictEqual("line one\nline two\nline three", "line one\nline 2\nline three"));
    it("throws error", () => {
        throw new TypeError("custom failure");
    });
    it("throws string", () => {
        throw "plain string";
    });
    it("throws object", () => {
        throw { code: 42 };
    });
    it("rejects", () => Promise.reject(new RangeError("rejected")));
    it("rejects undefined", () => Promise.reject());
    it("done with error", done => done(new Error("done failed")));
    it("done with string", done => done("a string"));
    it("error with cause", () => {
        throw new Error("outer", { cause: new Error("inner") });
    });
    it("passes after failures", () => {});
    it("assert.throws mismatch", () => assert.throws(() => { throw new Error("a"); }, /b/));
    it("assert.ok false", () => assert.ok(0 === 1));
    it("nested function frames", () => {
        function a() { b(); }
        function b() { c(); }
        function c() { throw new Error("deep"); }
        a();
    });
});

describe("hooks", function () {
    describe("before fails", () => {
        before(() => { throw new Error("before hook failed"); });
        it("never runs", () => {});
    });
    describe("afterEach fails", () => {
        afterEach(() => { throw new Error("afterEach hook failed"); });
        it("runs then hook fails", () => {});
        it("skipped after hook failure", () => {});
    });
});
