// RUN: 0
// Timeout objects: ref/unref/hasRef/refresh/close, Symbol.toPrimitive ids that
// clearTimeout accepts, and unref'd timers that do not keep the loop alive.
var log = [];
function out(s) { log.push(s); }
var t = setTimeout(function () { out("fired"); }, 10);
out("type " + typeof t + " " + t.constructor.name);
out("hasRef " + t.hasRef() + " " + t.unref().hasRef() + " " + (t.ref() === t) + " " + t.hasRef());
var id = +t;
out("id " + (typeof id) + " " + (id > 0) + " " + (+t === id) + " " + (t[Symbol.toPrimitive]() === id));
clearTimeout(id);
out("close " + (setTimeout(function () { out("never-closed"); }, 5).close().constructor.name));

var refreshed = setTimeout(function () { out("refreshed-fired"); }, 20);
setTimeout(function () { refreshed.refresh(); out("refresh-called"); }, 10);

var byString = setTimeout(function () { out("never-string-cleared"); }, 15);
clearTimeout(String(+byString));
clearTimeout(undefined);
clearTimeout(null);
clearTimeout({});

setTimeout(function () { out("unrefd-fired-while-alive"); }, 30).unref();
setInterval(function () { out("unrefd-interval-never"); }, 1000).unref();
setTimeout(function () { out("last-refd"); console.log(log.join("\n")); }, 60);
