// Measured 2026-10-10, jsse HEAD 2b621c5 vs boa 0.22.0, buildbox3, median of 5 (loadavg 10-12): jsse 1.72 s, boa 1.32 s, ratio 1.30x. CONTROL for the gap vs boa: boa is also O(remaining input) here; the point is jsse's absolute cost.
// Exercises: a sticky (/y) RegExp whose pattern needs lookahead (so it is not
// handled by the plain `regex` crate and runs in the backtracking VM), failing
// at lastIndex on a ~14 KB input that contains a would-be match far away.
// Models: js-tokens (Punctuator `(?!\d)`, `(?![\/*])`, StringLiteral `\1`...
// are all attempted at every token position and most attempts fail).
// A spec-correct sticky match inspects only position lastIndex. Measured:
// cost per failed exec grows with the length of the remaining input.
let src = "#".repeat(14000) + "5";
const withLookahead = /(?!0)\d/y;
let hits = 0;
for (let r = 0; r < 1500; r++) {
  withLookahead.lastIndex = r % 50;
  if (withLookahead.exec(src)) hits++;
}
console.log("sticky_lookahead", src.length, hits);
