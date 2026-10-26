// primitives, strings, numbers, symbols
showAll("prim", [undefined, null, true, false, 0, -0, 1, -1, 1.5, 1e21, 1e-7, NaN, Infinity, -Infinity,
  123456789, 0.1 + 0.2, 2 ** 53, -(2 ** 53), 12345678901234567890n, -5n, 0n, Symbol(), Symbol("a"), Symbol("a b"),
  Symbol.iterator, Symbol.for("reg"), Symbol("")]);
showAll("str", ["", "a", "it's", 'say "hi"', "both ' and \"", "back`tick", "all ' \" `", "line1\nline2", "tab\there",
  "\u0000\u0001\u001f\u007f\u0080\u009f", "\\", "\\n", "\ud800", "\udc00", "ok😀", "éü中文",
  "a".repeat(30), "a".repeat(100), "word ".repeat(30), "x\ny".repeat(5)]);
showAll("strlong", ["a".repeat(120)], { maxStringLength: 20 });
showAll("strlong2", ["a\nb\nc\n".repeat(30)], { breakLength: 40 });
showAll("strnested", [["long ".repeat(20) + "\nmore text\nthird line here and a quite long line to wrap around the break length"]]);
showAll("strobj", [{ s: "line1\nline2\nline3 is a bit longer than usual to force breaking onto multiple lines ok" }]);
showAll("numsep", [1234567, 1234567.1234567, -1234567, 12345678901234567890n, 0.0001234, 123456789.123456789, 1e21, 100, 1000], { numericSeparator: true });
showAll("numsep2", [[1000, 20000, 300000, 4000000], { n: 1234567 }, new Map([[1000000, 2000000]])], { numericSeparator: true });
showAll("boxed", [new Number(1), new Number(-0), new String("abc"), new Boolean(false), Object(1n), Object(Symbol("s")),
  Object.assign(new Number(3), { a: 1 }), Object.assign(new String("xy"), { b: 2 })]);
showAll("boxed-hidden", [new String("abc"), new Number(3)], { showHidden: true });
show("colors-prim", [1, "s", true, null, undefined, 3n, Symbol("c")], { colors: true });
show("colors-obj", { a: 1, b: "x", c: [true, null], d: new Date(0), e: /re/g, f() {}, g: Symbol("s"), h: undefined }, { colors: true });
show("colors-nested", { a: { b: { c: { d: 1 } } } }, { colors: true, depth: 0 });
show("colors-map", new Map([["k", { v: 1 }]]), { colors: true });
show("colors-string", ["it's", "plain", "a\nb"], { colors: true });
show("colors-regexp", [/ab+c/, /(?<n>x)|[a-z]\d{2,3}/gi, /^$/], { colors: true });
