// Deterministic differential corpus for the embedded url, querystring and
// punycode modules (#1230). The runner concatenates corpus-*.js in name order
// and runs the result on Node and on `jsse --node`: stdout must be
// byte-identical and its last line is pinned in corpus.digest.
var load = function (id) {
  return typeof __node_builtin === "function" ? __node_builtin(id) : require(id);
};
var url = load("url");
var querystring = load("querystring");
var punycode = load("punycode");
process.noDeprecation = true;
process.chdir("/");

var seed = 0x2545f491;
function rnd(n) {
  seed = (seed + 0x6d2b79f5) | 0;
  var t = Math.imul(seed ^ (seed >>> 15), 1 | seed);
  t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
  return ((t ^ (t >>> 14)) >>> 0) % n;
}
function pick(list) { return list[rnd(list.length)]; }

var out = [];
var hash = 0x811c9dc5;
function emit(line) {
  out.push(line);
  for (var i = 0; i < line.length; i++) {
    hash = Math.imul(hash ^ line.charCodeAt(i), 0x01000193) >>> 0;
  }
  hash = Math.imul(hash ^ 10, 0x01000193) >>> 0;
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
      input: e.input,
      base: e.base,
      ctor: e instanceof TypeError ? "TypeError" : e instanceof URIError ? "URIError" : e instanceof Error ? "Error" : typeof e,
      keys: Object.keys(e),
    });
  }
}

var URL_FIELDS = ["href", "origin", "protocol", "username", "password", "host", "hostname", "port", "pathname", "search", "hash"];
function fields(u) {
  var o = {};
  URL_FIELDS.forEach(function (k) { o[k] = u[k]; });
  return JSON.stringify(o);
}

var SCHEMES = ["http", "https", "ftp", "ws", "wss", "file", "blob", "data", "javascript", "mailto", "web+demo", "foo", "HTTP", "Http", "a b", "1x", "x-y.z", ""];
var SEPARATORS = ["://", ":/", ":", ":///", ":\\\\", ":/\\", "://\\", ":////"];
var USERINFO = ["", "", "", "u@", "u:p@", ":p@", "u:p:q@", "a%40b@", "@", "u@v@", "\u00fc@", " @", "u:@", "%zz:%@", "\u{1f600}:x@"];
var HOSTS = [
  "example.com", "EXAMPLE.COM", "ExAmPlE.com.", "127.0.0.1", "0x7f.1", "1.2.3", "4294967295", "4294967296",
  "0300.0250.0.1", "1.2.3.4.5", "1.2.3.", "0x", "08.1", "256.1.1.1", "1.2.3.256", "[::1]", "[1:2:3:4:5:6:7:8]",
  "[::ffff:1.2.3.4]", "[1::]", "[::]", "[1:0:0:2:0:0:0:3]", "[g::]", "[1:2:3:4:5:6:7:8:9]", "[1:2:3:4:5:6:1.2.3.4]",
  "[::1.2.3]", "[::01.2.3.4]", "[:1]", "[1:]", "[]", "::1", "[::1", "fa\u00df.de", "\u4f60\u597d.com", "xn--fiq228c.com",
  "xn--a.com", "xn--zca.com", "XN--ZCA.com", "xn--", "a b.com", "a%20b.com", "%41.com", "%e4%bd%a0.com", "a_b.com",
  "a.b.", "localhost", "", "a:b", "\u00ad", "\u00ada", "A\u0308.com", "\u01c6.com", "b\u00fccher.de", "%", "ab%zz", "a<b",
  "a^b", "a|b", "a\u200db", "\u0661\u0662.com", "\u05d0.com", "a.\u05d0", "\u3002", "a\u3002b", "\uff21.com", "1\uff0e2\uff0e3\uff0e4",
  "\u2488.com", "\ud7ff.com", "\ufffd.com", "\u202e.com", "a..b", ".", "..", "-a.com", "a-.com", "xn--a-.com", "r\u00e9sum\u00e9",
  "\ufdd0.com", "\u1e9e.com", "\u03c2.gr", "\u0905\u094d\u200d.com", "\u0f00.com", "\u{1f600}.com", "\u{e0001}.com",
];
var PORTS = ["", "", "", ":80", ":8080", ":0", ":65535", ":65536", ":abc", ":", ":080", ":443", ":21", ":99999999999", ":8a", ": 80", ":-1"];
var PATHS = [
  "", "/", "/a/b", "/a/./b/../c", "/%2e%2E/a", "/a b", "/\u00fc", "/a?b", "\\a\\b", "//", "/a//b", "/C:/x", "/C|/x", "/..",
  "/a/..", "/.", "/%2f", "/{}|^`", "/\u{1f600}", "/\ud800", "/a/b/", "/./", "/../..", "/%2E", "/.%2e/", "/a%", "/%zz",
  "/~!$&'()*+,;=:@", "/\u0000", "/\t/a\n", "/a\\..\\b", "/C:", "/c:/", "/C:/..", "/a/%2e./b", "a", "a/b", "../a", "./a",
];
var QUERIES = ["", "", "", "?", "?a=b", "?a b", "?'", "?\"<>#", "?\u00fc", "?%", "?a=1&b=2", "?\u{1f600}", "?`{}|^\\"];
var FRAGMENTS = ["", "", "", "#", "#a b", "#`<>", "#\u00fc", "#%", "#?x", "##", "#\u0000", "#\u{1f600}"];

function randomURL() {
  var scheme = pick(SCHEMES);
  var sep = rnd(3) === 0 ? pick(SEPARATORS) : "://";
  var s = scheme + sep;
  if (rnd(4) === 0) s += pick(USERINFO);
  s += pick(HOSTS) + pick(PORTS) + pick(PATHS);
  if (rnd(2)) s += pick(QUERIES);
  if (rnd(2)) s += pick(FRAGMENTS);
  return s;
}
function randomRelative() {
  var r = rnd(10);
  if (r < 2) return pick(PATHS) + pick(QUERIES) + pick(FRAGMENTS);
  if (r < 3) return "//" + pick(HOSTS) + pick(PORTS) + pick(PATHS);
  if (r < 4) return pick(QUERIES) + pick(FRAGMENTS);
  if (r < 5) return pick(FRAGMENTS);
  if (r < 6) return pick(SCHEMES) + ":" + pick(PATHS) + pick(QUERIES);
  return randomURL();
}
var BASES = [
  "http://example.com/a/b/c?x#y", "https://user:pw@h.example:8443/p/q/", "file:///C:/dir/file.txt", "file:///dir/sub/file",
  "file://host/share/x", "about:blank", "data:text/plain,hi", "mailto:a@b.c", "web+demo://h/p/q", "foo:/a/b", "foo://h",
  "http://[::1]:3000/", "javascript:void(0)", "blob:https://a.example/uuid", "ws://h/x", "ftp://f.example/dir/", "foo:a/b?c#d",
  "http://h/a b/", "file:///C:/", "file://localhost/x", "not a url", "",
];
