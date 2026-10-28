"use strict";

let ready = false;

describe("delayed", function () {
    it("runs only after run() is called", () => {
        if (!ready) throw new Error("started before run()");
    });
});

setTimeout(() => {
    ready = true;
    run();
}, 50);
