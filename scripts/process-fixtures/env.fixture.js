// ENV: FOO=bar EMPTY= SPACED=a_b_c
// RUN: 0
function show(label, value) {
  console.log(label + " " + JSON.stringify(value));
}
function attempt(label, fn) {
  try {
    show(label, fn());
  } catch (e) {
    show(label, ["throws", e.name, e.code === undefined ? null : e.code, e.message]);
  }
}
var env = process.env;
show("FOO", env.FOO);
show("EMPTY", env.EMPTY);
show("has EMPTY", "EMPTY" in env);
show("missing", env.NOPE_NOT_SET);
show("keys", Object.keys(env).filter(function (k) { return k !== "PATH"; }).sort());
attempt("set number", function () { env.N = 5; return [typeof env.N, env.N]; });
attempt("set undefined", function () { env.U = undefined; return [typeof env.U, env.U]; });
attempt("set null", function () { env.L = null; return env.L; });
attempt("set object", function () { env.O = {}; return env.O; });
attempt("set symbol value", function () { env.S = Symbol("x"); });
attempt("set symbol key", function () { env[Symbol("x")] = "v"; });
attempt("get symbol key", function () { return env[Symbol("x")] === undefined; });
attempt("delete present", function () { return [delete env.FOO, env.FOO, "FOO" in env]; });
attempt("delete missing", function () { return delete env.NOPE_NOT_SET; });
attempt("hasOwnProperty", function () { return Object.prototype.hasOwnProperty.call(env, "SPACED"); });
attempt("defineProperty ok", function () {
  Object.defineProperty(env, "D", { value: 7, configurable: true, writable: true, enumerable: true });
  return [typeof env.D, env.D];
});
attempt("defineProperty readonly", function () { Object.defineProperty(env, "D2", { value: "v" }); });
attempt("defineProperty accessor", function () {
  Object.defineProperty(env, "D3", { get: function () { return 1; }, configurable: true, enumerable: true });
});
attempt("descriptor", function () { return Object.getOwnPropertyDescriptor(env, "SPACED"); });
attempt("assign", function () { Object.assign(env, { Z: 1 }); return env.Z; });
attempt("integer key", function () { env[1] = 2; return env["1"]; });
attempt("for-in", function () {
  var out = [];
  for (var k in env) if (k === "SPACED" || k === "Z") out.push(k);
  return out.sort();
});
