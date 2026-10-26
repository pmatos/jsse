// Internationalized hosts: a code point sweep (every 53rd code point in the
// BMP and SMP, in three positions) and random labels from a pool of
// mapping, normalization, joiner and bidi edge cases.
(function () {
  // Code points where Node's IDNA tables (bidi classes and validity of
  // Unicode 14+ additions) and the idna crate's disagree; skipped.
  var STALE = [0x61d,0x61d,0x870,0x88f,0x897,0x89f,0x8b5,0x8b5,0x8c8,0x8d2,0xc3c,0xc3c,0xcf3,0xcf3,0xece,0xece,0x1715,0x1715,0x1ac1,0x1add,0x1ae0,0x1aeb,0x1dfa,0x1dfa,0xfbc2,0xfbc2,0x10940,0x10959,0x10d40,0x10d65,0x10d69,0x10d6d,0x10d6f,0x10d85,0x10d8e,0x10d8f,0x10ec2,0x10ec7,0x10efa,0x10eff,0x10f70,0x10f89,0x11070,0x11070,0x11073,0x11074,0x110c2,0x110c2,0x11241,0x11241,0x113b8,0x113c0,0x113c2,0x113c2,0x113c5,0x113c5,0x113c7,0x113ca,0x113cc,0x113d0,0x113d2,0x113d2,0x113e1,0x113e2,0x11b60,0x11b67,0x11f00,0x11f01,0x11f03,0x11f03,0x11f34,0x11f3a,0x11f3e,0x11f42,0x11f5a,0x11f5a,0x13440,0x13440,0x13447,0x13455,0x1611e,0x1612f,0x16d67,0x16d67,0x1cf00,0x1cf2d,0x1cf30,0x1cf46,0x1e08f,0x1e08f,0x1e2ae,0x1e2ae,0x1e4ec,0x1e4ef,0x1e5ee,0x1e5ef,0x1e6e3,0x1e6e3,0x1e6e6,0x1e6e6,0x1e6ee,0x1e6ef,0x1e6f5,0x1e6f5];
  function stale(cp) {
    for (var i = 0; i < STALE.length; i += 2) if (cp >= STALE[i] && cp <= STALE[i + 1]) return true;
    return false;
  }
  function host(h) {
    return summarize(function () { return new URL("http://" + h + "/").hostname; });
  }
  for (var cp = 0x80; cp < 0x20000; cp += 53) {
    if ((cp >= 0xd800 && cp < 0xe000) || stale(cp)) continue;
    var c = String.fromCodePoint(cp);
    emit("sweep " + cp.toString(16) + " " + host("a" + c + "b.com") + " " + host(c + ".org") + " " + host("x." + c + c));
  }
  var POOL = ["a", "B", "1", "-", "_", "\u00df", "\u03c2", "\u00e9", "e\u0301", "\u00c5", "A\u030a", "\u212b", "\u0130", "\u0131", "\u00ad", "\ufe0f",
    "\u05d0", "\u05d1", "\u0627", "\u0628", "\u0661", "\u06f1", "\u0915", "\u094d", "\u0e01", "\u3042", "\u30a2", "\u4e2d", "\uac00", "\u1100", "\u1161", "\u3002", "\uff0e", "\uff61",
    "\uff21", "\uff41", "\u2460", "\u2488", "\u2160", "\u00bd", "\ufb01", "\u1e9e", "\u0345", "\u0301", "\u0308", "\u1fbe", "\u2126", "\u2000", "\u202e", "\u200e", "\u0000", "%", "\ufdd0", "\ufffd", "\u{1f600}", "\u{10400}", "\u{1d7ce}"];
  for (var i = 0; i < 3000; i++) {
    var label = "";
    var n = 1 + rnd(5);
    for (var k = 0; k < n; k++) label += pick(POOL);
    var h = label + (rnd(2) ? "." + pick(["com", "\u05d0", "1", "xn--zca", "\u00fc"]) : "");
    emit("pool " + describe(h) + " => " + host(h) + " " + summarize(function () { return url.domainToASCII(h); }) + " " + summarize(function () { return url.domainToUnicode(h); }));
  }
})();
