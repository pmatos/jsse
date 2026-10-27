"use strict";
const assert = require("assert");

describe("pass", function () {
    const order = [];
    before(() => order.push("before"));
    after(() => order.push("after"));
    beforeEach(() => order.push("beforeEach"));
    afterEach(() => order.push("afterEach"));

    it("sync", () => assert.strictEqual(1 + 1, 2));
    it("promise", () => Promise.resolve().then(() => assert.ok(true)));
    it("async", async () => {
        await new Promise(r => setTimeout(r, 1));
        assert.deepStrictEqual({ a: [1, 2] }, { a: [1, 2] });
    });
    it("callback", done => setImmediate(done));
    it("nextTick", done => process.nextTick(done));
    it("generous timeout", function (done) {
        this.timeout(5000);
        setTimeout(done, 5);
    });
    it("order", () => assert.deepStrictEqual(order.slice(0, 3), ["before", "beforeEach", "afterEach"]));
    it.skip("skipped", () => assert.fail("never runs"));
    it("pending");

    describe("nested", () => {
        it("inner", () => assert.ok(order.length > 3));
        describe("deeper", () => {
            it("leaf", function () {
                assert.strictEqual(this.test.fullTitle(), "pass nested deeper leaf");
            });
        });
    });

    let attempts = 0;
    it("retried", function () {
        this.retries(2);
        if (++attempts < 3) throw new Error("flaky " + attempts);
    });

    for (const n of [1, 2, 3]) it("generated " + n, () => assert.ok(n > 0));

    it("process hooks stay registered", () => {
        assert.ok(process.listenerCount("uncaughtException") >= 1);
        assert.strictEqual(typeof process.hrtime.bigint(), "bigint");
        assert.strictEqual(process.exitCode, undefined);
    });
});
