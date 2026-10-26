// RUN: 0
function attempt(label, fn) {
  try { fn(); console.log(label, "no error"); }
  catch (e) { console.log(label, e.name, e.code, e.message); }
}
attempt("setTimeout", function () { setTimeout("x", 1); });
attempt("setInterval", function () { setInterval(undefined); });
attempt("setImmediate", function () { setImmediate(1); });
attempt("nextTick", function () { process.nextTick(null); });
attempt("queueMicrotask", function () { queueMicrotask({}); });
attempt("emitter", function () { process.on("x", 5); });
console.log(typeof setTimeout(function () {}, NaN).unref().hasRef());
console.log(setTimeout.length, setImmediate.length, clearTimeout.length);
