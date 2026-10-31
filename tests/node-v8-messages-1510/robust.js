// Errors the V8 wording must not replace or turn into a crash (#1510): a
// RangeError a valueOf throws keeps its identity, the JSON scanner that words
// syntax errors survives text nested far deeper than it follows, the callee
// scanner reads non-ASCII names, and state kept for one frame (pattern depth,
// the not-iterable error) is not taken for another's.
"use strict";
function takesPattern({ x }) {}
const café = 1;
const attempts = [
  [false, () => new ArrayBuffer({ valueOf() { throw new RangeError("mine too"); } })],
  [true, () => JSON.parse("[".repeat(200000))],
  [true, () => JSON.parse('{"a":'.repeat(5000) + "}")],
  [false, () => (café)()],
  [false, () => { const { a = takesPattern() } = {}; }],
  [false, () => { for (const x of { [Symbol.iterator]() { new Map(5); } }); }],
  [false, () => { Object.create(new Proxy({}, { set() { return false; } })).q = 1; }],
  [false, () => { const Uint8Array = {}; new Uint8Array(); }],
];
const out = [];
for (const [nameOnly, attempt] of attempts) {
  try {
    attempt();
    out.push("no error");
  } catch (e) {
    out.push(nameOnly ? e.name : e.name + ": " + e.message);
  }
}
console.log(out.join("\n"));
