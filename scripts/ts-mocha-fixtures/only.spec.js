"use strict";

describe("only", function () {
    it.only("runs alone", () => {});
    it("is left out", () => {});
    it("is pending");
});
