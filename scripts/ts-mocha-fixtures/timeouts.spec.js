"use strict";
const assert = require("assert");

describe("timeouts", function () {
    it("fires at its limit", function (done) {
        this.timeout(40);
    });
    it("fires for a promise that never settles", function () {
        this.timeout(40);
        return new Promise(() => {});
    });
    it("fires for an async function that never settles", async function () {
        this.timeout(40);
        await new Promise(() => {});
    });
    it("passes inside its limit", function (done) {
        this.timeout(2000);
        setTimeout(done, 30);
    });
    it("timeout(0) disables the limit", function (done) {
        this.timeout(0);
        setTimeout(done, 80);
    });
    it("a limit above 2^31-1 disables it", function (done) {
        this.timeout(2 ** 31);
        setTimeout(done, 50);
    });
    it("a limit raised mid-test applies", function (done) {
        this.timeout(30);
        setTimeout(() => this.timeout(500), 10);
        setTimeout(done, 80);
    });
    it("a limit lowered mid-test applies", function (done) {
        this.timeout(5000);
        setTimeout(() => this.timeout(20), 10);
        setTimeout(done, 200);
    });
    it("a synchronous overrun is reported after the test", function () {
        this.timeout(20);
        const end = Date.now() + 80;
        while (Date.now() < end);
    });
    it("done after the timeout is ignored", function (done) {
        this.timeout(20);
        setTimeout(done, 60);
    });
    it("a timer from a timed out test does not break the next", function (done) {
        setTimeout(done, 100);
    });
    it("reads its own limit back", function () {
        this.timeout(1234);
        assert.strictEqual(this.timeout(), 1234);
        assert.strictEqual(this.test.timeout(), 1234);
    });

    describe("hooks", function () {
        describe("before all times out", function () {
            before(function (done) {
                this.timeout(30);
            });
            it("is skipped", () => {});
        });
        describe("beforeEach times out", function () {
            beforeEach(function (done) {
                this.timeout(30);
            });
            it("is skipped", () => {});
            it("is skipped too", () => {});
        });
        describe("afterEach times out", function () {
            afterEach(function () {
                this.timeout(30);
                return new Promise(() => {});
            });
            it("runs then hook fails", () => {});
        });
        describe("after times out", function () {
            after(function (done) {
                this.timeout(30);
            });
            it("passes", () => {});
        });
        describe("a slow hook inside its limit", function () {
            before(function (done) {
                this.timeout(500);
                setTimeout(done, 30);
            });
            it("runs", () => {});
        });
    });

    describe("a suite limit", function () {
        this.timeout(30);
        it("is inherited", function (done) {});
        it("can be overridden", function (done) {
            this.timeout(500);
            setTimeout(done, 60);
        });
        describe("and nested suites", function () {
            it("inherit it", function (done) {});
        });
    });

    describe("retries", function () {
        this.retries(2);
        let once = 0;
        it("times out once then passes", function (done) {
            this.timeout(40);
            if (++once > 1) done();
        });
        it("times out on every attempt", function (done) {
            this.timeout(20);
        });
    });

    it("the run recovers", () => {});
});
