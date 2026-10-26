// WPT-style vectors (the shapes urltestdata.json and setters_tests.json
// exercise), written out by hand; Node supplies the expected values.
(function () {
  var BASES2 = ["about:blank", "http://example.org/foo/bar", "file:///tmp/mock/path", "http://www.example.com/test", "sc://x/", "data:text/plain,x", "http://a/b?c#d", "sc:/p/q", "http://[::1]/x"];
  var VECTORS = [
    "http://example\t.\norg", "http://user:pass@foo:21/bar;par?b#c", "https://test:@test", "https://:@test", "non-special://test:@test/x", "non-special://:@test/x",
    "http:foo.com", "\t   :foo.com   \n", " foo.com  ", "a:\t foo.com", "http://f:21/ b ? d # e ", "lolscheme:x x#x x", "http://f:/c", "http://f:0/c",
    "http://f:00000000000000/c", "http://f:00000000000000000000080/c", "http://f:b/c", "http://f: /c", "http://f:\n/c", "http://f:fifty-two/c", "http://f:999999/c",
    "non-special://f:999999/c", "http://f: 21 / b ? d # e ", "", " \t", ":foo.com/", ":foo.com\\", ":", ":a", ":/", ":\\", ":#", "#", "#/", "#\\", "#;?", "?", "/", ":23",
    "/:23", "\\x", "\\\\x\\hello", "::", "::23", "foo://", "http://a:b@/www.example.com", "http://user@/www.example.com", "http:@/www.example.com", "http:/@/www.example.com",
    "http://@/www.example.com", "https:@/www.example.com", "http:a:b@/www.example.com", "http:/a:b@/www.example.com", "http://a:b@/www.example.com", "http::@/www.example.com",
    "http:@:www.example.com", "http:/@:www.example.com", "http://@:www.example.com", "http://:@www.example.com", "/a/b/c", "/a/ /c", "/a%2fc", "/a/%2f/c", "\\\\foo\\bar",
    "http://foo.com/\\@", "http:\\\\foo.com\\", "http:\\\\a\\b:c\\d@foo.com\\", "foo:/", "foo:/bar.com/", "foo://///////", "foo://///////bar.com/", "foo:////://///",
    "c:/foo", "//foo/bar", "http://foo/path;a??e#f#g", "http://foo/abcd?efgh?ijkl", "http://foo/abcd#foo?bar", "[61:24:74]:98", "http:[61:27]/:foo", "http://[1::2]:3:4",
    "http://2001::1", "http://2001::1]", "http://2001::1]:80", "http://[::127.0.0.1]", "http://[0:0:0:0:0:0:13.1.68.3]", "http://[2001::1]:80", "http:/example.com/", "ftp:/example.com/",
    "https:/example.com/", "madeupscheme:/example.com/", "file:/example.com/", "file://example:1/", "file://example:test/", "file://example%/", "file://[example]/", "ftps:/example.com/",
    "gopher:/example.com/", "ws:/example.com/", "wss:/example.com/", "data:/example.com/", "javascript:/example.com/", "mailto:/example.com/", "http:example.com/", "ftp:example.com/",
    "https:example.com/", "madeupscheme:example.com/", "ftps:example.com/", "gopher:example.com/", "ws:example.com/", "wss:example.com/", "data:example.com/", "javascript:example.com/",
    "mailto:example.com/", "/a/b/c", "/a/ /c", "/a%2fc", "/a/%2f/c", "#\u03b2", "data:text/html,test#test", "tel:1234567890", "http://www.google.com/foo?bar=baz# \u00bb",
    "http://[www.google.com]/", "http://www.google.com./", "http://%25DOMAIN:foobar@foodomain.com/", "http:\\\\www.google.com\\foo", "http://foo:80/", "http://foo:81/",
    "httpa://foo:80/", "http://foo:-80/", "https://foo:443/", "https://foo:80/", "ftp://foo:21/", "ftp://foo:80/", "gopher://foo:70/", "gopher://foo:443/", "ws://foo:80/", "ws://foo:81/",
    "ws://foo:443/", "ws://foo:815/", "wss://foo:80/", "wss://foo:81/", "wss://foo:443/", "wss://foo:815/", "http:/example.com/", "http://example.com/foo/%2e", "http://example.com/foo/%2e%2",
    "http://example.com/foo/%2e./%2e%2e/.%2e/%2e.bar", "http://example.com////../..", "http://example.com/foo/bar//../..", "http://example.com/foo/bar//..", "http://example.com/foo",
    "http://example.com/%20foo", "http://example.com/foo%", "http://example.com/foo%2", "http://example.com/foo%2zbar", "http://example.com/foo%2\u00c2\u00a9zbar", "http://example.com/foo%41%7a",
    "http://example.com/foo\t\u0091%91", "http://example.com/foo%00%51", "http://example.com/(%28:%3A%29)", "http://example.com/%3A%3a%3C%3c", "http://example.com/foo\tbar", "http://example.com\\\\foo\\\\bar",
    "http://example.com/%7Ffp3%3Eju%3Dduvgw%3Dd", "http://example.com/@asdf%40", "http://example.com/\u4f60\u597d\u4f60\u597d", "http://example.com/\u2025/foo", "http://example.com/\ufeff/foo",
    "http://example.com/\u202e/foo/\u202d/bar", "http://www.google.com/foo?bar=baz#", "http://www.google.com/foo?bar=baz# \u00bb", "http://[::1.2.3.4]", "http://[::ffff:1.2.3.4]/",
    "http://[1:2:3:4:5:6:7:8]/", "http://[1:2:3:4:5:6:7:8:9]/", "http://[1:2:3:4:5:6:7]/", "http://[::1::2]/", "http://[1::2::3]/", "http://[:1:2:3:4:5:6:7]/", "http://[1:2:3:4:5:6:7:]/",
    "http://[::ffff:256.2.3.4]/", "http://[::1.2.3.4.5]/", "http://[::1.2.3]/", "http://[0:0:0:0:0:0:0:0]/", "http://[1:0:0:2:0:0:0:3]/", "http://[1:0:0:0:2:0:0:3]/", "http://[0:1:0:0:0:0:0:0]/",
    "http://0x7f.0x0.0x0.0x1/", "http://0300.0250.0.1/", "http://192.168.0.257/", "http://256/", "http://4294967295/", "http://4294967296/", "http://0xffffffff/", "http://0x100000000/test", "http://0x/",
    "http://0xg/", "http://1.2.3.4./", "http://1.2.3.4.../", "http://1..2/", "http://.1.2.3/", "http://foo.09/", "http://foo.09..", "http://foo.0x4", "http://foo.0XFfFfFfFfFfFfFfFfFfAcE123",
    "http://1.2.3.08", "http://1.2.3.08.", "http://0..0x300/", "http://%30%78%63%30%2e%30%32%35%30.01", "http://%30%78%63%30%2e%30%32%35%30.01%2e", "http://192.168.0.1 hello", "http://999999999999/",
    "file:c:\\foo\\bar.html", "  File:c|////foo\\bar.html", "C|/foo/bar", "/C|\\foo\\bar", "//C|/foo/bar", "//server/file", "\\\\server\\file", "/\\server/file", "file:///foo/bar.txt", "file:///home/me",
    "//", "///", "///test", "file://test", "file://localhost", "file://localhost/", "file://localhost/test", "test", "file:test", "file:///C:/", "file://C:/foo", "file:///C|/", "file:\\\\C:\\", "file:/C:/",
    "file://1.2.3.4/x", "file://[::1]/x", "file://xn--/x", "file:..", "file:..\\..", "file:/..", "file:///C:/../x", "file:///C:", "file:///C:..", "file:///C:/a/../..", "file:///a/../../C:/b", "file:///C:/../C:/x",
    "sc://\u00f1", "sc://\u00f1?x", "sc://\u00f1#x", "sc://%/", "sc://@/", "sc://te@s:t@/", "sc://:/", "sc://:12/", "sc://[/", "sc://\\/", "sc://]/", "sc:\\../", "sc::a@example.net", "sc://a b/", "sc://a<b", "sc://a>b",
    "sc://a^b", "sc://a|b", "sc://a%2Fb", "sc://a%3Ab", "sc://\u0000/", "sc://\u007f/", "sc://\u00a0/", "sc:/.//", "sc:/..//a", "sc:/a/..//p", "sc://h/./..//p", "foo://host/..//p", "web+demo:/.//not-a-host/",
    "web+demo:/path/..//not-a-host/", "web+demo:/path/..//not-a-host/../..", "non-spec:/.//", "non-spec:/..//", "non-spec:/a/..//", "non-spec:/.//path", "non-spec:/..//path", "non-spec:/a/..//path", "non-special:cannot-be-a-base-url-path",
    "data:space ?query", "data:space   ?query", "sc:space   ?query", "data:space   #hash", "sc:space   #hash", "data:space   ", "sc:space   ", "data: x", "data:x ", "data:  ?x#y", "javascript:alert(1) ", "javascript:  ",
    "x:\u0000y", "x:\u001fy", "x:\u007fy", "x:\u00a0y", "x:%zz", "x:y z?q r#f g", "view-source+http://example.org/foo", "ssh://example.com/foo/bar.git", "http://example.com:8080:80/", "https://\u00a0",
    "http://\u0000", "http://a\u0000b", "http://a\tb.c\nd", "http://ab%2e.com", "http://%2e.com", "http://a%2eb", "http://%zz.com", "http://a%00b", "http://%ef%bb%bfa.com", "http://\ufeffa.com", "http://a\u00adb.com",
    "http://\u0430\u0431\u0432.\u0440\u0444", "http://\u2603.net", "http://\u00df.de", "http://fa\u00df.de", "http://\u03c2.com", "http://\u1e9e.com", "http://xn--zca.com", "http://\u1d2f.com", "http://\u2160.com", "http://\u2490.com",
    "http://\u200d.com", "http://a\u200d.com", "http://\u0915\u094d\u200d.com", "http://\u0915\u094d\u200c.com", "http://a.\u0301", "http://\u0301.a", "http://\u0661a", "http://a\u0661", "http://\u05d0\u0661", "http://\u05d0a",
    "http://a.\u05d0", "http://\u05d0.a", "http://\u05d0.1", "http://1.\u05d0", "http://\u0627\u0644\u0639\u0631\u0628\u064a\u0629.com", "http://\u30c9\u30e1\u30a4\u30f3.\u30c6\u30b9\u30c8", "http://\u4e2d\u6587.\u4e2d\u56fd",
    "http://\uff4a\uff41\uff56\uff41.com", "http://a\uff0eb", "http://a\u3002b", "http://a\uff61b", "http://\u00c4.com", "http://a\u0308.com", "http://\u00e4.com", "http://\u2488.com", "http://\ud83d\ude00.com", "http://\u{1d400}.com",
    "http://a_b.com", "http://-a.com", "http://a-.com", "http://a--b.com", "http://ab--.com", "http://xn--a.com", "http://xn--.com", "http://" + "a".repeat(64) + ".com", "http://" + "a".repeat(260),
  ];
  VECTORS.forEach(function (v) {
    emit("vector " + describe(v) + " => " + summarize(function () { return fields(new URL(v)); }));
    BASES2.forEach(function (b) {
      emit("vector " + describe(v) + " base " + describe(b) + " => " + summarize(function () { return new URL(v, b).href; }));
    });
  });

  var SETS = [
    ["protocol", "a://example.net", ["", "b", "b:", "b:foo", "b:\u00fc", "http", "https", "file", "ftp", "javascript", "view-source+http", "0b", "b0", "b+-.", "b ", "b\u00e9"]],
    ["protocol", "http://example.net", ["https", "ftp", "ws", "file", "foo", "HTTPS:", "wss"]],
    ["protocol", "file:///x", ["http", "foo", "file"]],
    ["protocol", "http://u:p@example.net:8080/x", ["file", "https", "ws", "foo"]],
    ["protocol", "http://example.net:443/", ["https", "wss"]],
    ["protocol", "mailto:a@b", ["http", "foo"]],
    ["username", "http://example.net", ["", "me", "m e", "m\u00e9", "a:b", "a@b", "\u0000", "%41"]],
    ["password", "http://me@example.net", ["", "secret", "s:e", "s@e", "\u00fc", "%41"]],
    ["username", "file:///x", ["me"]], ["username", "sc:///x", ["me"]], ["username", "sc://h/x", ["me"]], ["password", "mailto:a@b", ["x"]],
    ["host", "http://example.net/x", ["", "x", "x:8080", "x:", "x:abc", "x:99999", "[::1]", "[::1]:80", "a b", "a\\b", "a/b", "a?b", "a#b", "\u00fc.com", "1.2.3", "0x7f.1", "x:80/y", "x\u0000"]],
    ["host", "sc://h/x", ["", "x", "x:8080", "a b", "a%20b", "\u00fc"]],
    ["host", "file:///x", ["", "x", "localhost", "x:80", "C:", "\u00fc"]],
    ["host", "mailto:a@b", ["x"]], ["hostname", "http://example.net:8080/x", ["x", "x:90", "", "[::1]", "a b", "x/y"]],
    ["hostname", "sc://h:1/x", ["", "x", "x:90"]], ["hostname", "file:///x", ["x", "", "localhost"]],
    ["port", "http://example.net/x", ["", "80", "8080", "65535", "65536", "-1", "abc", "80abc", "  80", "80  ", "0", "00080", "443", "\u0661"]],
    ["port", "https://example.net:8080/x", ["", "443", "80"]], ["port", "file:///x", ["80"]], ["port", "sc://h/x", ["80", ""]], ["port", "sc:///x", ["80"]], ["port", "mailto:a@b", ["80"]],
    ["pathname", "http://example.net/x", ["", "/", "a", "/a/b", "/a/../b", "/a b", "/a?b", "/a#b", "\\a\\b", "/%2e", "/\u00fc", "//x", "/a/%2e%2E/b", "..", "."]],
    ["pathname", "sc://h/x", ["", "a", "/a", "//a", "/.//a", "\\a"]], ["pathname", "sc:/x", ["//a", "a", ""]], ["pathname", "mailto:a@b", ["x"]],
    ["pathname", "file:///C:/x", ["/D:/y", "/..", "D|/y", "../../z"]], ["pathname", "data:text/plain,x", ["/y"]],
    ["search", "http://example.net/x?y#z", ["", "?", "a", "?a", "??a", "a b", "a#b", "'", "\u00fc", "a=1&b=2"]], ["search", "sc://h/x", ["a'", "b c"]],
    ["search", "data:text/plain,x   ", ["a", ""]], ["search", "data:text/plain,x   #f", ["a", ""]], ["search", "data:text/plain,x   ?q", [""]],
    ["hash", "http://example.net/x?y#z", ["", "#", "a", "#a", "##a", "a b", "a`b", "\u00fc", "\u0000"]], ["hash", "data:text/plain,x   ?q", ["", "h"]], ["hash", "data:x   #f", [""]],
    ["href", "http://example.net/x", ["http://a/", "nope", "", "file:///x", "sc:x", "mailto:a@b"]],
  ];
  SETS.forEach(function (row) {
    row[2].forEach(function (value) {
      emit("setter " + row[0] + " " + describe(value) + " on " + describe(row[1]) + " => " + summarize(function () {
        var u = new URL(row[1]);
        u[row[0]] = value;
        return fields(u);
      }));
    });
  });
})();
