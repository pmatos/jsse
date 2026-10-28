"use strict";

describe("retry", function () {
    let calls = 0;
    beforeEach(() => console.log("beforeEach", ++calls));
    afterEach(() => console.log("afterEach", calls));

    let flaky = 0;
    it("fails twice then passes", () => {
        if (++flaky < 3) throw new Error("attempt " + flaky);
    });
    it("always fails", () => {
        throw new Error("never passes");
    });
    it("passes the first time", () => {});
    it("an explicit count wins over the option", function () {
        this.retries(0);
        throw new Error("no retries here");
    });
});
