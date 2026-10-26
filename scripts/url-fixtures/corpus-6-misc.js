// File URLs, domainTo*, urlToHttpOptions, querystring and punycode.
(function () {
  var POSIX = ["/", "/a/b", "/a b/c%d", "/a#b?c", "/\u00fc", "/a\\b", "/a\nb\r\t", "rel/x", "", ".", "..", "/a/../b", "/trailing/", "/C:/x", "/%2F", "/\u{1f600}", "/a[b]{c}", "/~user", "/a'b\"c", "/%00", "//double", "/a%25b"];
  var WIN = ["C:\\x\\y", "c:/x/y", "C:\\", "\\\\srv\\share\\f", "\\\\srv\\share", "\\\\?\\C:\\x", "\\\\?\\UNC\\srv\\share\\f", "\\\\.\\pipe\\x", "\\\\", "\\\\\\a", "C:", "C:rel", "\\x", "x\\y", "C:\\a b\\#c", "\\\\srv\\sh%are\\f", "\\\\\u00fc\\x\\y", "\\\\a b\\c\\d", "\\\\[::1]\\x\\y"];
  POSIX.concat(WIN).forEach(function (p) {
    [true, false].forEach(function (windows) {
      var r = summarize(function () { return url.pathToFileURL(p, { windows: windows }).href; });
      emit("pathToFileURL " + describe(p) + " windows=" + windows + " => " + r);
      var m = /^"(.*)"$/.exec(r);
      if (m) {
        var href = JSON.parse(r);
        emit("  roundtrip " + summarize(function () { return url.fileURLToPath(href, { windows: windows }); }));
        emit("  roundtrip URL " + summarize(function () { return url.fileURLToPath(new URL(href), { windows: windows }); }));
      }
    });
  });
  var FILES = ["file:///a/b", "file:///a%20b", "file://host/a", "file://localhost/a", "file:///C:/x/y", "file:///c:/", "file://srv/share/f", "file:///a%2Fb", "file:///a%5Cb", "file:///%E4%BD%A0", "file:///%FF", "file:///x/%", "file://xn--fiq228c.com/p", "file:///", "file:", "file:///C|/x", "file:///1:/x", "file:////x", "http://a/b", "file://[::1]/x", "file:///a?q#h", "file:///C:/a/%2e%2e/b"];
  FILES.forEach(function (f) {
    [true, false].forEach(function (windows) {
      emit("fileURLToPath " + f + " windows=" + windows + " => " + summarize(function () { return url.fileURLToPath(f, { windows: windows }); }) +
        " " + summarize(function () { return url.fileURLToPath(new URL(f), { windows: windows }); }));
    });
  });
  [undefined, null, 1, {}, { href: "file:///x", protocol: "file:" }, new URL("file:///z")].forEach(function (v) {
    emit("fileURLToPath odd => " + summarize(function () { return url.fileURLToPath(v, { windows: false }); }) + " pathToFileURL " + summarize(function () { return url.pathToFileURL(v).href; }));
  });
  ["", "example.com", "EXAMPLE.com", "fa\u00df.de", "\u4f60\u597d", "xn--fiq228c", "xn--zca.com", "a b", "a:80", "a/b", "a\u00adb", "\u2488", "[::1]", "1.2.3.4", "0x7f.1", "a..b", "xn--a", "\u202e", "a%41", "ab.\u05d0", "-a", "\uff21.com", "\u3002", "x".repeat(300), "\u4f60".repeat(70) + ".com"].forEach(function (d) {
    emit("domain " + describe(d) + " => " + summarize(function () { return url.domainToASCII(d); }) + " " + summarize(function () { return url.domainToUnicode(d); }));
  });
  emit("domain odd " + summarize(function () { return url.domainToASCII(); }) + " " + summarize(function () { return url.domainToUnicode(undefined); }) + " " + summarize(function () { return url.domainToASCII(1.5); }));
  ["http://u:p%41@h.com:81/p?q#h", "http://[::1]/", "http://a/", "http://%E4@a/", "file:///x", "http://h:80/x"].forEach(function (s) {
    emit("urlToHttpOptions " + s + " => " + summarize(function () { return JSON.stringify(url.urlToHttpOptions(new URL(s))); }));
  });
  emit("urlToHttpOptions plain " + summarize(function () { return JSON.stringify(url.urlToHttpOptions({ protocol: "http:", hostname: "[::1]", port: "9", pathname: "/p", search: "?q", extra: 1 })); }) + " " + summarize(function () { return url.urlToHttpOptions(1); }));

  var QS = ["", "a=1&b=2", "a=1&a=2&a=3", "a&b=&=c", "a=b=c", "%41=%42", "a+b=c+d", "a=%ff&b=%zz&c=%", "%e4%bd%a0=1", "&&&", "a=1;b=2", "a:1,b:2", "a=1&b=2&c=3&d=4", "x=%", "\u00fc=\u00fd", "a[]=1&a[]=2", "=", "a=b&", "k=v=w=x"];
  QS.forEach(function (s) {
    [undefined, { maxKeys: 2 }, { maxKeys: 0 }, { decodeURIComponent: function (x) { return x.toUpperCase(); } }].forEach(function (o, i) {
      emit("qs.parse " + describe(s) + " #" + i + " => " + summarize(function () { var r = querystring.parse(s, undefined, undefined, o); return JSON.stringify(r) + Object.getPrototypeOf(r); }));
    });
    emit("qs.parse sep " + describe(s) + " => " + summarize(function () { return JSON.stringify(querystring.parse(s.replace(/&/g, ";").replace(/=/g, ":"), ";", ":")); }) +
      " " + summarize(function () { return JSON.stringify(querystring.parse(s, "&&", "=="));}));
  });
  [{ a: 1, b: "x y", c: [1, 2], d: true, e: null, f: undefined, g: {}, h: 1n, i: NaN, j: Infinity, k: "\u00fc", l: "\ud800", m: "\u{1f600}", n: [], o: "a&b=c" }, {}, "str", 5, null, undefined, [1, 2], { "a b": "c d" }, { a: ["x"], b: [] }].forEach(function (obj) {
    emit("qs.stringify " + summarize(function () { return querystring.stringify(obj); }) + " " + summarize(function () { return querystring.stringify(obj, ";", ":"); }) +
      " " + summarize(function () { return querystring.stringify(obj, null, null, { encodeURIComponent: function (s) { return "<" + s + ">"; } }); }));
  });
  ["a b", "a+b", "%41%zz", "%e4%bd", "\u00fc", "a%2", "%", "%%41", "~!*()'-_.", "\u{1f600}", "\ud800", "A%2fB"].forEach(function (s) {
    emit("qs.escape " + describe(s) + " => " + summarize(function () { return querystring.escape(s); }) + " unescape " + summarize(function () { return querystring.unescape(s); }) +
      " unescapeBuffer " + summarize(function () { return querystring.unescapeBuffer(s).toString("hex"); }) + " " + summarize(function () { return querystring.unescapeBuffer(s, true).toString("hex"); }));
  });
  emit("qs odd " + summarize(function () { return querystring.escape({ toString: function () { return "t t"; } }); }) + " " + summarize(function () { return querystring.escape(Symbol("s")); }) +
    " " + summarize(function () { return querystring.escape(1.5); }) + " " + summarize(function () { return querystring.parse(5); }) + " " + summarize(function () { return Object.keys(querystring).join(); }));

  ["", "abc", "m\u00fcnchen", "\u4f60\u597d", "a\u00dfb", "\u{1f600}", "ab-c", "-", "\u00fc-", "bcher-kva"].forEach(function (s) {
    emit("punycode " + describe(s) + " => " + summarize(function () { return punycode.encode(s); }) + " " + summarize(function () { return punycode.decode(s); }) +
      " " + summarize(function () { return punycode.toASCII(s); }) + " " + summarize(function () { return punycode.toUnicode(s); }));
  });
  ["xn--mnchen-3ya.de", "mailto:\u00fc@\u00fc.com", "a.b.\u00fc", "xn--zz", "\u4f60\u597d\u3002com"].forEach(function (s) {
    emit("punycode dom " + describe(s) + " => " + summarize(function () { return punycode.toASCII(s); }) + " " + summarize(function () { return punycode.toUnicode(s); }));
  });
  emit("punycode ucs2 " + summarize(function () { return [punycode.ucs2.decode("a\u{1f600}\ud800b"), punycode.ucs2.encode([97, 0x1f600, 0xd800, 98]), punycode.version]; }));
})();
