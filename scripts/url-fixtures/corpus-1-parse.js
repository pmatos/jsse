// Parsing: absolute inputs, relative inputs against bases, canParse, URL.parse.
(function () {
  var seen = 0;
  for (var i = 0; i < 4500; i++) {
    var input = randomURL();
    emit("parse " + describe(input) + " => " + summarize(function () { return fields(new URL(input)); }));
    seen++;
  }
  for (var j = 0; j < 4500; j++) {
    var rel = randomRelative();
    var base = pick(BASES);
    emit("parse " + describe(rel) + " base " + describe(base) + " => " +
      summarize(function () { return fields(new URL(rel, base)); }) + " " +
      summarize(function () { return URL.canParse(rel, base); }));
  }
  BASES.forEach(function (base) {
    PATHS.concat(QUERIES, FRAGMENTS, ["", " ", "\t", "  http://x  ", "\u0000a:b", "//", "///", "////x"]).forEach(function (rel) {
      emit("relative " + describe(rel) + " base " + describe(base) + " => " +
        summarize(function () { return new URL(rel, base).href; }));
    });
  });
  [undefined, null, 1, true, {}, [], { toString: function () { return "http://t.example/"; } }].forEach(function (v) {
    emit("odd input " + describe(v) + " => " + summarize(function () { return new URL(v).href; }) +
      " " + summarize(function () { return new URL("x", v).href; }) +
      " " + summarize(function () { return URL.canParse(v); }));
  });
  emit("no args " + summarize(function () { return new URL(); }) + " " + summarize(function () { return URL.canParse(); }) +
    " " + summarize(function () { return URL.parse(); }));
  emit("URL.parse " + summarize(function () { return URL.parse("http://a/b?c").href; }) + " " + summarize(function () { return URL.parse("nope"); }) +
    " " + summarize(function () { return URL.parse("b", "http://a/").href; }));
  emit("stringify " + summarize(function () { var u = new URL("http://a/b"); return [String(u), JSON.stringify(u), u.toJSON(), u + ""]; }));
  emit("brand " + summarize(function () { try { Object.getOwnPropertyDescriptor(URL.prototype, "href").get.call({}); } catch (e) { return e.name; } }));
  emit("tag " + Object.prototype.toString.call(new URL("http://a/")) + " " + Object.prototype.toString.call(new URLSearchParams()));
  emit("keys " + JSON.stringify(Object.keys(URL.prototype)) + " " + JSON.stringify(Object.getOwnPropertyNames(URL).filter(function (n) { return !/ObjectURL$/.test(n); })) +
    " " + URL.length + " " + URL.name + " " + URLSearchParams.length + " " + URL.canParse.length + " " + URL.parse.length);
})();
