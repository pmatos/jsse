"use strict";

process.on("exit", code => {
    process.stdout.write("exit hook: " + code + "\n");
});
process.on("beforeExit", () => {
    if (!globalThis.__beforeExitSeen) {
        globalThis.__beforeExitSeen = true;
        process.stdout.write("beforeExit hook\n");
    }
});

describe("exit", function () {
    it("passes", () => {});
    it("fails", () => { throw new Error("exit code check"); });
    it("fails too", () => { throw new Error("second failure"); });
});
