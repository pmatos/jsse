"use strict";
const assert = require("assert");

const trace = [];

suite("tdd", function () {
    suiteSetup(() => trace.push("suiteSetup"));
    suiteTeardown(() => trace.push("suiteTeardown"));
    setup(() => trace.push("setup"));
    teardown(() => trace.push("teardown"));

    test("runs its hooks", () => assert.deepStrictEqual(trace, ["suiteSetup", "setup"]));
    test("fails", () => assert.strictEqual(trace.length, 99));
    test("times out", function (done) {
        this.timeout(20);
    });
    test.skip("is skipped", () => {});
});
