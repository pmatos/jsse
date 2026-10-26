// RUN: 0
// `timers` and `timers/promises` (served from the registry until `require`
// exists in the host).
var timers, promises;
if (typeof require === "function") {
  timers = require("timers");
  promises = require("timers/promises");
} else {
  var registry = globalThis[Symbol.for("jsse.node.builtins")];
  timers = registry.timers;
  promises = registry["timers/promises"];
}
var log = [];
function out(s) { log.push(s); }
out("same " + [
  timers.setTimeout === setTimeout, timers.clearTimeout === clearTimeout,
  timers.setInterval === setInterval, timers.setImmediate === setImmediate,
  timers.clearImmediate === clearImmediate, timers.promises === promises,
].join());
(async function () {
  out("value " + await promises.setTimeout(5, "v"));
  out("undef " + await promises.setTimeout(1));
  out("imm " + await promises.setImmediate("iv"));
  out("wait " + await promises.scheduler.wait(1));
  await promises.scheduler.yield();
  var n = 0;
  for await (var v of promises.setInterval(3, "tick")) {
    out("interval " + v + " " + (++n));
    if (n === 3) break;
  }
  try { await promises.setTimeout(1, null, { signal: {} }); }
  catch (e) { out("signal " + e.code); }
  try { await promises.setTimeout(1, null, { ref: "x" }); }
  catch (e) { out("ref " + e.code); }
  var unrefd = promises.setTimeout(10000, null, { ref: false });
  out("done");
  console.log(log.join("\n"));
})();
