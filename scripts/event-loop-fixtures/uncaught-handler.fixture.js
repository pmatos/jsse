// RUN: 0
// uncaughtException handlers receive errors from timers, immediates, ticks,
// microtasks and promise rejections; the loop carries on. A throwing tick
// abandons the current checkpoint, so the microtask runs after the immediate.
var seen = [];
process.on("uncaughtException", function (err, origin) {
  seen.push(err.message + "@" + origin);
});
process.nextTick(function () { throw new Error("tick"); });
queueMicrotask(function () { throw new Error("microtask"); });
setTimeout(function () { throw new Error("timer"); }, 20);
setImmediate(function () { throw new Error("immediate"); });
setTimeout(function () { process.nextTick(function () { console.log("after: loop alive"); }); }, 40);
Promise.reject(new Error("rejection"));
setTimeout(function () { console.log(seen.join("\n")); }, 80);
