// Errors the V8 wording must not replace or turn into a crash (#1510): a
// RangeError a valueOf throws keeps its identity, and the JSON scanner that
// words syntax errors survives text nested far deeper than it follows.
const out = [];
const attempts = [
  () => new ArrayBuffer({ valueOf() { throw new RangeError("mine too"); } }),
  () => JSON.parse("[".repeat(200000)),
  () => JSON.parse('{"a":'.repeat(5000) + "}"),
];
for (const attempt of attempts) {
  try {
    attempt();
    out.push("no error");
  } catch (e) {
    out.push(e.name + (e instanceof RangeError ? ": " + e.message : ""));
  }
}
console.log(out.join("\n"));
