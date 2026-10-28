"use strict";

describe("leak", function () {
    it("leaks a global", () => {
        globalThis.leakedByTheTest = 1;
    });
    it("leaks an allowed global", () => {
        globalThis.allowedLeak = 1;
    });
    it("is clean", () => {});
    after(() => {
        delete globalThis.leakedByTheTest;
        delete globalThis.allowedLeak;
    });
});
