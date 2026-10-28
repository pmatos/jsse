"use strict";

describe("slow", function () {
    this.slow(1);
    it("is flagged slow", done => setTimeout(done, 15));
});
