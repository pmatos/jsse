"use strict";

// Takes 250 ms: passes under the default 40 s timeout and fails under --timeout=60.
describe("timeout", function () {
    it("sleeps a quarter of a second", done => setTimeout(done, 250));
    it("is quick", () => {});
});
