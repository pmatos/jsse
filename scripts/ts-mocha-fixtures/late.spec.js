"use strict";

describe("late", function () {
    it("finishes first", () => {
        setTimeout(() => { throw new Error("thrown after the test finished"); }, 5);
    });
    it("is running when the late error lands", function (done) {
        this.timeout(2000);
        setTimeout(done, 100);
    });
});
