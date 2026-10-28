"use strict";

describe("cli timeout", function () {
    it("finishes at once", done => setTimeout(done, 1));
    it("waits 150ms", done => setTimeout(done, 150));
    it("waits 150ms and allows it", function (done) {
        this.timeout(2000);
        setTimeout(done, 150);
    });
    it("is fast for its threshold", function (done) {
        this.slow(100000);
        setTimeout(done, 5);
    });
    it("is slow for its threshold", function (done) {
        this.slow(10);
        this.timeout(0);
        setTimeout(done, 120);
    });
});
