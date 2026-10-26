// Deterministic differential corpus for the embedded util module (#1224). The
// runner concatenates corpus-*.js in name order and runs the result on Node
// and on `jsse --node`: stdout must be byte-identical and its last line is
// pinned in corpus.digest. Error stacks, timings and the process environment
// are never printed: errors get a fixed .stack, or are shown by code/message.
var load = function (id) {
  return typeof __node_builtin === "function" ? __node_builtin(id) : require(id);
};
var util = load("util");
var inspect = util.inspect;
var types = util.types;
process.noDeprecation = true;

var out = [];
var hash = 0x811c9dc5;
function emit(line) {
  line = String(line);
  out.push(line);
  for (var i = 0; i < line.length; i++) {
    hash = Math.imul(hash ^ line.charCodeAt(i), 0x01000193) >>> 0;
  }
  hash = Math.imul(hash ^ 10, 0x01000193) >>> 0;
}
function show(label, value, options) {
  var text;
  try {
    text = inspect(value, options);
  } catch (e) {
    text = "throws " + e.name + " " + (e.code || "") + " " + e.message;
  }
  emit(label + " => " + text);
}
function showAll(label, values, options) {
  values.forEach(function (value, i) { show(label + "[" + i + "]", value, options); });
}
function fmt(label) {
  var args = Array.prototype.slice.call(arguments, 1);
  var text;
  try {
    text = util.format.apply(null, args);
  } catch (e) {
    text = "throws " + e.name + " " + (e.code || "") + " " + e.message;
  }
  emit(label + " => " + text);
}
function attempt(label, fn) {
  try {
    var value = fn();
    emit(label + " => " + (typeof value === "string" ? value : inspect(value)));
  } catch (e) {
    emit(label + " => throws " + e.name + " " + (e.code || "") + " " + e.message);
  }
}
function fakeError(Ctor, message, props) {
  var e = new Ctor(message);
  e.stack = Ctor.name + (message === undefined ? "" : ": " + message) + "\n    at fake (/app/file.js:1:1)\n    at other (/app/file.js:2:2)";
  if (props) Object.keys(props).forEach(function (k) { e[k] = props[k]; });
  return e;
}
var seed = 0x2545f491;
function rnd(n) {
  seed = (seed + 0x6d2b79f5) | 0;
  var t = Math.imul(seed ^ (seed >>> 15), 1 | seed);
  t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
  return ((t ^ (t >>> 14)) >>> 0) % n;
}
function pick(list) { return list[rnd(list.length)]; }
