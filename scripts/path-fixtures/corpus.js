// Deterministic differential corpus for the embedded path module. Prints one
// line per call plus a final "digest" line; the output must be byte-identical
// on Node and on jsse --node (scripts/run-node-path-oracle.sh), and the digest
// is pinned in corpus.digest (tests/node_path.rs).
var path = require("path");
process.chdir("/");

var seed = 0x9e3779b9;
function rnd(n) {
  seed = (seed + 0x6d2b79f5) | 0;
  var t = Math.imul(seed ^ (seed >>> 15), 1 | seed);
  t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
  return (((t ^ (t >>> 14)) >>> 0) % n);
}
function pick(list) { return list[rnd(list.length)]; }

var SEGMENTS = [
  "", ".", "..", "a", "b", "foo", "bar.txt", "x.y.z", ".hidden", "..x", "x..",
  "a.", ".", "é", " ", "C:", "c:", "D:foo", "C:\\", "c:/", "//srv/share",
  "\\\\srv\\share\\x", "\\\\?\\", "\\\\?\\C:\\a", "\\\\.\\pipe", "\\\\.\\C:",
  "CON", "con:", "COM1:", "LPT\u00b9:", "NUL", "a:b", "UNC", "share",
];
var SEPARATORS = ["/", "\\", "//", "\\\\", "/./", "\\..\\", "/../", "", "/", "\\"];
var SUFFIXES = ["", ".txt", "txt", ".js", "a", "b.txt", "x.y.z", "\\a", "/a", "z"];
var ODD = [undefined, null, 0, 1, true, false, {}, [], function f() {}, 1n, NaN, "x"];

function randomPath() {
  var n = rnd(6);
  var out = "";
  for (var i = 0; i <= n; i++) {
    if (i > 0) out += pick(SEPARATORS);
    out += pick(SEGMENTS);
  }
  return out;
}
function randomArg() {
  var r = rnd(40);
  if (r === 0) return pick(ODD);
  return randomPath();
}

function describe(value) {
  if (typeof value === "bigint") return value + "n";
  if (typeof value === "function") return "function " + value.name;
  if (value === undefined) return "undefined";
  if (typeof value === "number" && value !== value) return "NaN";
  return JSON.stringify(value);
}
function summarize(fn) {
  try {
    return describe(fn());
  } catch (e) {
    return "throws " + JSON.stringify({
      name: e.name,
      code: e.code,
      message: e.message,
      ctor: e instanceof TypeError ? "TypeError" : e instanceof Error ? "Error" : typeof e,
      keys: Object.keys(e),
      str: String(e),
    });
  }
}

var out = [];
var hash = 0x811c9dc5;
function emit(line) {
  out.push(line);
  for (var i = 0; i < line.length; i++) {
    hash = Math.imul(hash ^ line.charCodeAt(i), 0x01000193) >>> 0;
  }
  hash = Math.imul(hash ^ 10, 0x01000193) >>> 0;
}

var parsedKeys = ["root", "dir", "base", "name", "ext"];
function randomObject(flavor) {
  var o = {};
  parsedKeys.forEach(function (k) {
    if (rnd(3) !== 0) {
      o[k] = rnd(12) === 0 ? pick(ODD) : pick(rnd(2) ? SEGMENTS : SUFFIXES.concat(["/", "\\", "C:\\", ""]));
    }
  });
  return o;
}

var GLOBS = [
  "*", "**", "**/*", "*.js", "**/*.js", "a/*", "a/**", "a/**/b", "{a,b}/*",
  "a/{b,c}/*.js", "[ab]/c", "[!a]/c", "?/b", "??", "a/?", "@(a|b)/c", "!(a)/b",
  "+(a|b)", "*(a)", "a/[b-d]", "a/**/**/b", "a/./b", "../a", "/a/*", "\\a\\*",
  "a\\b", "a/b/", "{1..3}", "{a,b,{c,d}}", "*.{js,ts}", ".*", "**/.*", "a/b/c",
  "", "[", "a[", "(", "a/(b|c)", "**/*/x",
];
var GLOB_PATHS = [
  "a", "a/b", "a/b/c", "a/b/c.js", "b/c", "c/b", "a.js", "x/y.js", ".hidden",
  "a/.hidden", "a/./b", "a\\b", "a\\b\\c.js", "/a/b", "\\a\\b", "1", "2",
  "../a", "", "a/b/", "a/b/c/d/e", "foo.ts", "a/c/z.js",
];

var flavors = { posix: path.posix, win32: path.win32 };
Object.keys(flavors).forEach(function (name) {
  var p = flavors[name];
  emit(name + " keys " + Object.keys(p).join());
  emit(name + " sep " + JSON.stringify([p.sep, p.delimiter, p.posix === path.posix, p.win32 === path.win32]));
  ["resolve", "normalize", "isAbsolute", "join", "relative", "toNamespacedPath", "dirname",
    "basename", "extname", "parse", "format", "matchesGlob"].forEach(function (m) {
    emit(name + " " + m + " " + p[m].length + " " + p[m].name);
  });
  var i;
  for (i = 0; i < 1500; i++) {
    var a = randomArg(), b = randomArg(), c = randomArg();
    emit(name + ".resolve " + describe(a) + " " + describe(b) + " => " + summarize(function () { return p.resolve(a, b); }));
    emit(name + ".resolve3 " + describe(a) + " " + describe(b) + " " + describe(c) + " => " + summarize(function () { return p.resolve(a, b, c); }));
    emit(name + ".join " + describe(a) + " " + describe(b) + " " + describe(c) + " => " + summarize(function () { return p.join(a, b, c); }));
    emit(name + ".normalize " + describe(a) + " => " + summarize(function () { return p.normalize(a); }));
    emit(name + ".isAbsolute " + describe(a) + " => " + summarize(function () { return p.isAbsolute(a); }));
    emit(name + ".relative " + describe(a) + " " + describe(b) + " => " + summarize(function () { return p.relative(a, b); }));
    emit(name + ".toNamespacedPath " + describe(a) + " => " + summarize(function () { return p.toNamespacedPath(a); }));
    emit(name + ".dirname " + describe(a) + " => " + summarize(function () { return p.dirname(a); }));
    emit(name + ".extname " + describe(a) + " => " + summarize(function () { return p.extname(a); }));
    var sfx = rnd(8) === 0 ? pick(ODD) : rnd(2) ? pick(SUFFIXES) : undefined;
    emit(name + ".basename " + describe(a) + " " + describe(sfx) + " => " + summarize(function () { return p.basename(a, sfx); }));
    var parsed = summarize(function () { return p.parse(a); });
    emit(name + ".parse " + describe(a) + " => " + parsed);
    if (parsed.charAt(0) === "{") {
      emit(name + ".roundtrip " + describe(a) + " => " + summarize(function () { return p.format(p.parse(a)); }));
    }
    var obj = randomObject(name);
    emit(name + ".format " + summarize(function () { return JSON.stringify(obj); }) + " => " + summarize(function () { return p.format(obj); }));
  }
  var unary = [undefined, null, 0, 1, true, {}, [], "", "x"];
  ["resolve", "normalize", "isAbsolute", "join", "relative", "toNamespacedPath", "dirname",
    "basename", "extname", "parse", "format"].forEach(function (m) {
    unary.forEach(function (v) {
      emit(name + "." + m + "(odd) " + describe(v) + " => " + summarize(function () { return p[m](v); }));
    });
    emit(name + "." + m + "() => " + summarize(function () { return p[m](); }));
  });
  GLOBS.forEach(function (glob) {
    GLOB_PATHS.forEach(function (candidate) {
      emit(name + ".matchesGlob " + JSON.stringify(candidate) + " " + JSON.stringify(glob) + " => " + summarize(function () { return p.matchesGlob(candidate, glob); }));
    });
  });
  [[1, "a"], ["a", 1], [undefined, "a"], ["a", null]].forEach(function (args) {
    emit(name + ".matchesGlob(odd) => " + summarize(function () { return p.matchesGlob(args[0], args[1]); }));
  });
});

emit("default platform export is a flavor: " + (path === path.posix || path === path.win32));
emit("_makeLong " + (path.posix._makeLong === path.posix.toNamespacedPath) + " " + (path.win32._makeLong === path.win32.toNamespacedPath));
emit("module aliases " + JSON.stringify(Object.keys(path)));

console.log(out.join("\n"));
console.log("digest " + hash.toString(16) + " " + out.length);
