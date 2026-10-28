"use strict";
const assert = require("assert");

const ms = since => Number(process.hrtime.bigint() - since) / 1e6;
const sleep = n => new Promise(r => setTimeout(r, n));

describe("timer queue", function () {
    this.timeout(10000);

    afterEach(function () {
        const want = this.currentTest.expectedMs;
        if (want === undefined) return;
        const got = this.currentTest.duration;
        assert.ok(got >= want - 2, `duration ${got} is shorter than ${want}ms`);
        assert.ok(got < want + 5000, `duration ${got} is far longer than ${want}ms`);
    });

    it("a timer does not fire early", function (done) {
        this.test.expectedMs = 50;
        const t0 = process.hrtime.bigint();
        setTimeout(() => {
            assert.ok(ms(t0) >= 48, `fired after ${ms(t0)}ms`);
            done();
        }, 50);
    });

    it("timers fire in deadline order, then creation order", function (done) {
        const seen = [];
        setTimeout(() => seen.push("a30"), 30);
        setTimeout(() => seen.push("b10"), 10);
        setTimeout(() => seen.push("c20"), 20);
        setTimeout(() => seen.push("d10"), 10);
        setTimeout(() => {
            assert.deepStrictEqual(seen, ["b10", "d10", "c20", "a30"]);
            done();
        }, 60);
    });

    it("microtasks, nextTick, timers and immediates run in phase order", function (done) {
        const seen = [];
        setTimeout(() => {
            seen.push("timeout");
            setTimeout(() => seen.push("timeout 2"), 0);
            setImmediate(() => seen.push("immediate"));
            process.nextTick(() => seen.push("tick in timer"));
            Promise.resolve().then(() => seen.push("promise in timer"));
        }, 5);
        process.nextTick(() => seen.push("tick"));
        Promise.resolve().then(() => seen.push("promise"));
        queueMicrotask(() => seen.push("microtask"));
        setTimeout(() => {
            assert.deepStrictEqual(seen, [
                "tick",
                "promise",
                "microtask",
                "timeout",
                "tick in timer",
                "promise in timer",
                "immediate",
                "timeout 2",
            ]);
            done();
        }, 60);
    });

    it("setInterval repeats until cleared", function (done) {
        this.test.expectedMs = 100;
        const t0 = process.hrtime.bigint();
        let n = 0;
        const id = setInterval(() => {
            if (++n === 3) {
                clearInterval(id);
                assert.ok(ms(t0) >= 58, `three ticks took ${ms(t0)}ms`);
                setTimeout(() => {
                    assert.strictEqual(n, 3);
                    done();
                }, 40);
            }
        }, 20);
    });

    it("clearTimeout cancels a pending timer", function (done) {
        let fired = false;
        const id = setTimeout(() => { fired = true; }, 10);
        clearTimeout(id);
        setTimeout(() => {
            assert.strictEqual(fired, false);
            done();
        }, 40);
    });

    it("a timer can cancel a later one", function (done) {
        let fired = false;
        const late = setTimeout(() => { fired = true; }, 40);
        setTimeout(() => clearTimeout(late), 5);
        setTimeout(() => {
            assert.strictEqual(fired, false);
            done();
        }, 70);
    });

    it("a chain of zero-delay timers advances", function (done) {
        const t0 = process.hrtime.bigint();
        let n = 0;
        (function next() {
            if (++n < 5) return setTimeout(next, 0);
            assert.ok(ms(t0) >= 3, `five hops took ${ms(t0)}ms`);
            done();
        })();
    });

    it("timer handles expose ref, unref, refresh, hasRef", function () {
        const t = setTimeout(() => {}, 1000);
        assert.strictEqual(t.hasRef(), true);
        assert.strictEqual(t.unref(), t);
        assert.strictEqual(t.hasRef(), false);
        assert.strictEqual(t.ref().hasRef(), true);
        assert.strictEqual(t.refresh(), t);
        assert.strictEqual(typeof +t, "number");
        clearTimeout(t);
    });

    it("refresh pushes the deadline back", function (done) {
        this.test.expectedMs = 60;
        const t0 = process.hrtime.bigint();
        const t = setTimeout(() => {
            assert.ok(ms(t0) >= 58, `fired after ${ms(t0)}ms`);
            done();
        }, 40);
        setTimeout(() => t.refresh(), 20);
    });

    it("sequential awaits add up", async function () {
        this.test.expectedMs = 60;
        const t0 = process.hrtime.bigint();
        await sleep(20);
        await sleep(20);
        await sleep(20);
        assert.ok(ms(t0) >= 58, `three sleeps took ${ms(t0)}ms`);
    });

    it("concurrent sleeps overlap", async function () {
        const t0 = process.hrtime.bigint();
        await Promise.all([sleep(40), sleep(40), sleep(40)]);
        assert.ok(ms(t0) >= 38 && ms(t0) < 5000);
    });

    it("timers/promises resolves with its value", async function () {
        const tp = require("timers/promises");
        assert.strictEqual(await tp.setTimeout(20, "value"), "value");
        assert.strictEqual(await tp.setImmediate("now"), "now");
    });

    it("timers/promises rejects when aborted", async function () {
        const tp = require("timers/promises");
        const ac = new AbortController();
        const p = tp.setTimeout(5000, null, { signal: ac.signal });
        setTimeout(() => ac.abort(), 10);
        await assert.rejects(p, { name: "AbortError", code: "ABORT_ERR" });
    });

    it("a rejected sleep inside a test is observable", async function () {
        await assert.rejects(
            new Promise((_, rej) => setTimeout(() => rej(new Error("late")), 10)),
            /late/
        );
    });

    it("Date and hrtime move together", async function () {
        const d0 = Date.now();
        const h0 = process.hrtime.bigint();
        await sleep(50);
        const dd = Date.now() - d0;
        const dh = ms(h0);
        assert.ok(Math.abs(dd - dh) < 25, `Date.now moved ${dd}ms, hrtime ${dh}ms`);
    });
});
