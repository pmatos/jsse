// RUN: 0
// nextTick drains before promise jobs; a tick queued from a promise job waits
// for the whole microtask queue; ticks queued by ticks run in the same drain.
var log = [];
function out(s) { log.push(s); }
out("sync-1");
Promise.resolve().then(function () {
  out("promise-1");
  process.nextTick(function () { out("tick-from-promise"); });
  Promise.resolve().then(function () { out("promise-from-promise"); });
});
process.nextTick(function () {
  out("tick-1");
  process.nextTick(function () { out("tick-from-tick"); });
  Promise.resolve().then(function () { out("promise-from-tick"); });
});
queueMicrotask(function () { out("microtask-1"); });
process.nextTick(function (a, b) { out("tick-args " + a + " " + b); }, 1, "two");
(async function () {
  out("async-start");
  await null;
  out("async-after-await");
  process.nextTick(function () { out("tick-from-async"); });
  await null;
  out("async-after-await-2");
})();
out("sync-2");
setTimeout(function () {
  out("timer");
  process.nextTick(function () { out("timer-tick"); });
  Promise.resolve().then(function () { out("timer-promise"); });
}, 1);
setTimeout(function () { out("timer-2"); console.log(log.join("\n")); }, 5);
