const value = { s: "x", n: 1.5, b: true, z: null, u: undefined, f() {}, arr: [undefined, () => 1, NaN, -0], d: new Date(0) };
show(JSON.stringify(value));
show(JSON.stringify(value, null, 2));
show(JSON.stringify(value, ["s", "arr"]), JSON.stringify({ a: 1, b: 2 }, (k, v) => (k === "b" ? undefined : v)));
show(JSON.stringify({ toJSON() { return { replaced: 1 }; } }), JSON.stringify("\u2028\ud800"), JSON.stringify(1n === 1n));
show(JSON.parse('{"a":[1,2,{"b":null}],"__proto__":1}').__proto__, JSON.parse("  [1e3, -0]  "));
show(JSON.parse('{"a":1,"b":{"c":2}}', (k, v) => (typeof v === "number" ? v * 10 : v)));
for (const bad of ["{a:1}", "[1,]", "'x'", "01", ""]) {
  try {
    JSON.parse(bad);
    show("parsed", bad);
  } catch (e) {
    show(e.name, bad);
  }
}
