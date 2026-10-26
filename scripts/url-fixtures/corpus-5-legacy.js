// Legacy url.parse / url.format / url.resolve / url.resolveObject.
(function () {
  var LEGACY = [
    "http://user:pass@host.com:8080/p/a/t/h?query=string#hash", "//foo/bar", "/a/b?c=d#e", "a/b", "mailto:a@b.c", "javascript:alert(1)",
    "HTTP://EXAMPLE.COM/A", "http://a@b@c/", "http://a@b?@c", "http://[::1]:3000/x", "http://x.com\\y", "  http://x.com/ y  ",
    "http://\u4f60\u597d.com/\u00fc?\u00fc#\u00fc", "file:///etc/passwd", "file://h/x", "http://a b.com/", "http://a.com:abc/", "http://a.com:/x",
    "x:y:z", "git+ssh://git@github.com:o/r.git", "http://", "http:///x", "?q=1", "#h", "", "http://x/a?b#c?d#e", "http://x/\t\n/y",
    "//a@b/c", "http://a.com/\"'<>`{}|^\\", "tel:+1-555", "http://%41.com/", "HtTp://X.cOm/#", "ftp://f/g", "gopher://g/h", "ws://w/x",
    "http://xn--fiq228c.com/", "http://a;b.com/", "http://a/b;c", "javascript://h/x", "blob:http://a/b", "data:,x", "http://a.b.c.d.e/",
    "http://0x7f.1/", "http://a.com./", "http://-a.com/", "c:\\path\\x", "\\\\srv\\share", "//[::1]/x", "http://[abc",
  ];
  var FLAGS = [[false, false], [true, false], [false, true], [true, true]];
  LEGACY.concat(Array.from({ length: 400 }, randomURL)).forEach(function (input, index) {
    var flags = index < LEGACY.length ? FLAGS : [FLAGS[rnd(4)]];
    flags.forEach(function (f) {
      emit("parse " + describe(input) + " " + f + " => " + summarize(function () {
        var p = url.parse(input, f[0], f[1]);
        var o = {};
        Object.keys(p).forEach(function (k) { o[k] = k === "query" && typeof p.query === "object" && p.query !== null ? JSON.stringify(p.query) + Object.getPrototypeOf(p.query) : p[k]; });
        return JSON.stringify(o) + " | " + url.format(p) + " | " + (p instanceof url.Url);
      }));
    });
  });
  for (var i = 0; i < 600; i++) {
    var from = rnd(3) === 0 ? pick(LEGACY) : randomURL();
    var to = rnd(2) ? randomRelative() : pick(LEGACY);
    emit("resolve " + describe(from) + " " + describe(to) + " => " + summarize(function () { return url.resolve(from, to); }) +
      " " + summarize(function () { return JSON.stringify(url.resolveObject(from, to)); }));
  }
  [
    { protocol: "http", host: "a.com", pathname: "/x", search: "y=1", hash: "h" }, { hostname: "a.com", port: 80, pathname: "x", query: { a: [1, 2], b: "c d" } },
    { protocol: "mailto:", auth: "a:b", host: "c.d" }, { protocol: "file", slashes: true, pathname: "/etc" }, { hostname: "::1", port: "8", path: "/p" },
    { pathname: "/a?b#c", search: "?d#e" }, {}, { href: "ignored", host: "h" }, { protocol: "javascript", pathname: "alert(1)" }, { auth: "u@v:w x", hostname: "h" },
    { query: "a=b" }, { query: null, search: "" }, { search: "a" }, { hash: "a" },
  ].forEach(function (obj) {
    emit("format " + JSON.stringify(obj) + " => " + summarize(function () { return url.format(obj); }));
  });
  [undefined, null, 1, true, [], function () {}].forEach(function (v) {
    emit("format odd " + describe(v) + " => " + summarize(function () { return url.format(v); }) + " parse " + summarize(function () { return url.parse(v); }));
  });
  ["http://u:p@xn--fiq228c.com:81/p?q#h", "http://a/", "file:///x", "http://\u00fc.com/"].forEach(function (s) {
    [{}, { fragment: false }, { unicode: true }, { search: false }, { auth: false }, { fragment: false, search: false, auth: false, unicode: true }, { unicode: 0, fragment: 0 }].forEach(function (o) {
      emit("format URL " + s + " " + JSON.stringify(o) + " => " + summarize(function () { return url.format(new URL(s), o); }));
    });
  });
  emit("format URL bad options " + summarize(function () { return url.format(new URL("http://a/"), 5); }));
})();
