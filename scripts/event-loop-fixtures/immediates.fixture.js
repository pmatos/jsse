// RUN: 0
// Immediates run in arming order in the check phase; one queued by another
// waits for the next iteration; ticks and promise jobs run between them.
var log = [];
function out(s) { log.push(s); }
var a = setImmediate(function (x, y) {
  out("imm-a " + x + y);
  setImmediate(function () { out("imm-nested"); });
  process.nextTick(function () { out("imm-a-tick"); });
  Promise.resolve().then(function () { out("imm-a-promise"); });
}, 1, 2);
setImmediate(function () { out("imm-b " + (this === b)); });
var b = setImmediate(function () { out("imm-c"); });
var cancelled = setImmediate(function () { out("never"); });
clearImmediate(cancelled);
clearImmediate(undefined);
clearImmediate({});
out("has-ref " + a.hasRef() + " " + a.unref().hasRef() + " " + a.ref().hasRef());
out("is-immediate " + a.constructor.name);
setTimeout(function () {
  setImmediate(function () { out("imm-in-timer"); });
  setTimeout(function () { out("timer-in-timer"); }, 0);
}, 5);
setTimeout(function () { console.log(log.join("\n")); }, 100);
