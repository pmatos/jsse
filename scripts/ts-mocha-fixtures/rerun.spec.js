"use strict";
const assert = require("assert");

// Fails until RERUN_FIXED is set, so a second run with --failed can tell what it re-ran.
const fixed = !!process.env.RERUN_FIXED;

describe("rerun", function () {
    it("stays green", () => {});
    it("stays green (a.b) [c]", () => {});
    it("breaks (1+1) [until fixed] {x}", () => assert.ok(fixed, "not fixed"));
    it("breaks | pipe $ caret ^ star *", () => assert.ok(fixed, "not fixed"));

    describe("nested + suite", () => {
        it("stays green too", () => {});
        it("breaks back\\slash ? plus +", () => assert.ok(fixed, "not fixed"));
    });
});

describe("rerun hooks", function () {
    before(() => assert.ok(fixed, "hook not fixed"));
    it("covered by the hook", () => {});
    it("also covered by the hook", () => {});
});
