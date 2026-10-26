// Every URL setter against random URLs and values.
(function () {
  var SETTERS = ["href", "protocol", "username", "password", "host", "hostname", "port", "pathname", "search", "hash"];
  var VALUES = {
    href: function () { return randomURL(); },
    protocol: function () { return pick(["http", "https:", "ftp", "file", "ws:", "wss", "foo", "foo:bar", "HTTP:", "a b", "1x", "", ":", "http://", "blob", "mailto", "x+y.z-w", "\u00fc"]); },
    username: function () { return pick(["", "u", "a b", "u:v", "@", "/", "\u00fc", "%41", "\u{1f600}", "a;b=c", "[]|\\"]); },
    password: function () { return pick(["", "p", "a b", "p:q", "@", "/", "\u00fc", "%41", "\u{1f600}", "a;b=c", "[]|\\"]); },
    host: function () { return pick(HOSTS) + pick(PORTS); },
    hostname: function () { return pick(HOSTS) + pick(["", "", ":80", "/x", "?q", "#h", "\\x"]); },
    port: function () { return pick(["", "80", "8080", "0", "65535", "65536", "abc", "80abc", "-1", " 80", "8 0", "443", "21", "1e3", "\u0661", "99999999999", "080"]); },
    pathname: function () { return pick(PATHS) + pick(["", "", "?x", "#y", "%3F"]); },
    search: function () { return pick(QUERIES.concat(["a", "?a", "??", "a#b", "a b", "\u00fc", "?"])); },
    hash: function () { return pick(FRAGMENTS.concat(["a", "#a", "##", "a b", "\u00fc", "#"])); },
  };
  for (var i = 0; i < 6000; i++) {
    var input = rnd(3) === 0 ? pick(BASES) : randomURL();
    var setter = pick(SETTERS);
    var value = VALUES[setter]();
    emit("set " + setter + " " + describe(value) + " on " + describe(input) + " => " + summarize(function () {
      var u = new URL(input);
      u[setter] = value;
      return fields(u);
    }));
  }
  emit("set odd " + summarize(function () {
    var u = new URL("http://a/b");
    u.port = 81;
    u.hash = null;
    u.search = undefined;
    u.pathname = { toString: function () { return "/o"; } };
    return u.href;
  }));
  emit("set href invalid " + summarize(function () { var u = new URL("http://a/"); u.href = "nope"; return u.href; }));
  emit("searchParams sync " + summarize(function () {
    var u = new URL("http://a/?x=1&y=2#h");
    var sp = u.searchParams;
    sp.set("x", "3");
    var a = u.href;
    u.search = "?q=1";
    var b = sp.toString();
    u.href = "http://b/?z=9";
    return [a, b, sp.toString(), u.searchParams === sp];
  }));
})();
