// Top-level `for (let ...)` with a block-scoped `let` in the body, next to a
// global lexical declaration (which stays a tree-walker unit).
const LIMIT = 2000000;
let total = 0;
for (let i = 0; i < LIMIT; i++) {
  let sq = (i * i) & 0xffff;
  total = (total + sq) | 0;
}
if (total === 0x7fffffff) throw new Error("unreachable");
