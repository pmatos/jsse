// RUN: 3
// process.exit() inside a timer emits exit synchronously, skips pending work,
// and exit handlers may adjust process.exitCode.
process.on("exit", function (code) {
  console.log("exit " + code);
  process.exitCode = 3;
});
process.on("beforeExit", function () { console.log("never: beforeExit is skipped by exit()"); });
setTimeout(function () { console.log("never"); }, 50);
setImmediate(function () {
  console.log("immediate");
  process.nextTick(function () { console.log("never-tick"); });
  Promise.resolve().then(function () { console.log("never-promise"); });
  process.exit(0);
  console.log("unreachable");
});
