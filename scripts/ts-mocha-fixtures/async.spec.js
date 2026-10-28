"use strict";
const assert = require("assert");

describe("async", function () {
    it("timeout never resolves", function (done) {
        this.timeout(30);
    });
    it("timeout promise never settles", function () {
        this.timeout(30);
        return new Promise(() => {});
    });
    it("done called twice", done => {
        done();
        done();
    });
    it("done and promise", function (done) {
        return Promise.resolve();
    });
    it("uncaught in nextTick", done => {
        process.nextTick(() => { throw new Error("uncaught in nextTick"); });
    });
    it("uncaught in timer", done => {
        setTimeout(() => { throw new Error("uncaught in timer"); }, 1);
    });
    it("uncaught non-error", done => {
        setImmediate(() => { throw 7; });
    });
    it("unhandled rejection", done => {
        Promise.reject(new Error("unhandled rejection"));
        setTimeout(done, 20);
    });
    it("unhandled rejection of a non-error", done => {
        Promise.reject("not an error");
        setTimeout(done, 20);
    });
    it("recovers", () => assert.ok(true));
});
