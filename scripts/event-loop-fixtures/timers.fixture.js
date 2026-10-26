// RUN: 0
// Timer ordering: shortest first, equal delays in arming order, 0 behaves as 1,
// arguments are passed, ticks and promise jobs run between timer callbacks.
var log = [];
function out(s) { log.push(s); }
setTimeout(function () { out("t30"); }, 30);
setTimeout(function () { out("t10-a"); }, 10);
setTimeout(function () { out("t10-b"); }, 10);
setTimeout(function () { out("t20"); }, 20);
setTimeout(function (a, b, c) { out("args " + [a, b, c].join()); }, 12, "x", 2, [3]);
setTimeout(function () {
  out("t1-a");
  process.nextTick(function () { out("t1-a-tick"); });
  Promise.resolve().then(function () { out("t1-a-promise"); });
}, 1);
setTimeout(function () { out("t1-b"); }, 1);
setTimeout(function () { out("t0-after-1"); }, 0);
var cleared = setTimeout(function () { out("never"); }, 5);
clearTimeout(cleared);
var self = setTimeout(function () {
  out("this-is-timeout " + (this === self) + " " + this.constructor.name);
}, 3);
var count = 0;
var iv = setInterval(function () {
  count++;
  out("interval " + count);
  if (count === 3) clearInterval(iv);
}, 4);
setTimeout(function () {
  out("nested-start");
  setTimeout(function () { out("nested-timeout"); }, 1);
  setImmediate(function () { out("nested-immediate"); });
}, 60);
setTimeout(function () { console.log(log.join("\n")); }, 200);
