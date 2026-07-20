// Self-test for a repeated completion callback scheduled by setInterval in the
// describe/it TAP runner in node-test-harness.js. The interval clears itself on
// its first tick; final TAP emission must wait for that tick and reject the late
// duplicate without treating the interval as indefinitely pending work.
//
// Expected summary: PASS: 0  FAIL: 1  TOTAL: 1

it("fails when a final interval repeats done", function (done) {
  done();
  var id = setInterval(function () {
    clearInterval(id);
    done();
  }, 5);
});
