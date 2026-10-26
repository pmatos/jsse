// URLSearchParams construction, mutation, iteration and serialization.
(function () {
  var KEYS = ["a", "b", "c", "a b", "\u00fc", "", "%", "+", "=", "&", "\u{1f600}", "\ud800", "x y", "a;b"];
  var INITS = [
    undefined, "", "?", "a=1", "?a=1&b=2", "a=1&a=2", "a", "a=", "=b", "&&", "a=b=c", "a+b=c+d", "%41=%42", "%zz=%", "%e4%bd%a0=1",
    "a=%ff", "\u00fc=\u00fd", "?a=1#b", "a=1&b", [["a", "1"], ["b", "2"]], [["a", "1", "x"]], [["a"]], [], {}, { a: "1", b: "2" },
    new Map([["m", "n"]]), new URLSearchParams("q=1"), "a=1&b=2&c=3&d=4", null, 5, true, ["ab", "cd"], [["a", "1"], 5],
  ];
  INITS.forEach(function (init) {
    emit("params init " + (typeof init === "object" && init !== null && !Array.isArray(init) ? Object.prototype.toString.call(init) : describe(init)) +
      " => " + summarize(function () {
        var sp = new URLSearchParams(init);
        return [sp.toString(), sp.size, Array.from(sp)];
      }));
  });
  for (var i = 0; i < 1500; i++) {
    var ops = [];
    var sp = new URLSearchParams(pick(["", "a=1&b=2&a=3", "x=1", "q=%C3%BC&r=a+b"]));
    var log = [];
    for (var j = 0; j < 8; j++) {
      var op = pick(["append", "set", "delete", "get", "getAll", "has", "sort", "deleteV", "hasV"]);
      var k = pick(KEYS);
      var v = pick(KEYS);
      ops.push(op + ":" + k + ":" + v);
      switch (op) {
        case "append": sp.append(k, v); break;
        case "set": sp.set(k, v); break;
        case "delete": sp.delete(k); break;
        case "deleteV": sp.delete(k, v); break;
        case "get": log.push(sp.get(k)); break;
        case "getAll": log.push(sp.getAll(k)); break;
        case "has": log.push(sp.has(k)); break;
        case "hasV": log.push(sp.has(k, v)); break;
        case "sort": sp.sort(); break;
      }
    }
    emit("params ops " + JSON.stringify(ops) + " => " + JSON.stringify(log) + " " + sp.toString() + " " + sp.size);
  }
  var big = new URLSearchParams();
  for (var n = 0; n < 150; n++) big.append(String.fromCharCode(97 + ((n * 7) % 26)) + (n % 5), String(n));
  big.sort();
  emit("params big sort " + big.toString());
  emit("params iteration " + summarize(function () {
    var sp = new URLSearchParams("a=1&b=2&c=3");
    var seen = [];
    sp.forEach(function (v, k, o) { seen.push(k + v + (o === sp)); if (k === "a") sp.append("d", "4"); });
    var it = sp.entries();
    var first = it.next();
    return [seen, first, Array.from(sp.keys()), Array.from(sp.values()), Object.prototype.toString.call(it),
      typeof it[Symbol.iterator], sp[Symbol.iterator] === sp.entries, Array.from(it)];
  }));
  emit("params errors " + summarize(function () { return new URLSearchParams().append("a"); }) + " " +
    summarize(function () { return new URLSearchParams().get(); }) + " " +
    summarize(function () { return new URLSearchParams().forEach(1); }) + " " +
    summarize(function () { return URLSearchParams.prototype.get.call({}, "a"); }) + " " +
    summarize(function () { return URLSearchParams.prototype.size; }) + " " +
    summarize(function () { return new URLSearchParams(Symbol.iterator); }) + " " +
    summarize(function () { return new URLSearchParams({ [Symbol.iterator]: 5 }); }));
  emit("params shape " + JSON.stringify(Object.keys(URLSearchParams.prototype)) + " " +
    JSON.stringify(Reflect.ownKeys(URLSearchParams.prototype).map(String)));
})();
