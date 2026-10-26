// RUN: 0
// beforeExit fires when the loop drains and may schedule more work (then
// fires again); exit fires last with the exit code and may change it.
var rounds = 0;
process.on("beforeExit", function (code) {
  console.log("beforeExit " + code + " round " + rounds);
  if (rounds++ < 2) {
    setTimeout(function () { console.log("more work " + rounds); }, 1);
  }
});
process.on("exit", function (code) {
  console.log("exit " + code + " " + process.exitCode);
  setTimeout(function () { console.log("never: timers do not run in exit"); }, 0);
  process.nextTick(function () { console.log("never: ticks do not run in exit"); });
});
process.once("exit", function () { console.log("once-exit"); });
console.log("listeners " + process.listenerCount("exit") + " " + process.listenerCount("beforeExit"));
