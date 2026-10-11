// Measured 2026-10-10, jsse HEAD 2b621c5 vs boa 0.22.0, buildbox3, median of 5 (loadavg 10-12): jsse 2.98 s, boa 1.22 s, ratio 2.44x, jsse --no-bytecode (median of 3) 2.98 s.
// Exercises: sticky (/y) RegExp exec that FAILS at lastIndex on a long input
// where the pattern would match later in the string. The js-tokens tokenizer
// tries ~10 sticky regexes per token and most of those attempts fail.
// Models: js-tokens.
// A spec-correct sticky match only tries position lastIndex. jsse runs these
// patterns as an unanchored search from lastIndex and rejects a match that does
// not start there; the lookahead/backtracking patterns also run in the fancy-regex
// VM and pay per-exec constant costs (pattern/flags re-conversion, subject
// re-conversion). boa is also O(remaining input) on failed sticky execs, so the
// gap here is mostly the per-exec constant and the VM, not the scan alone.
let src = "";
for (let i = 0; i < 400; i++) src += "var identifier" + i + " = value + other;\n";
src += "/* trailing comment */ // end\n";
const reComment = /\/\*(?:[^*]|\*(?!\/))*(\*\/)?/y;
const reLine = /\/\/.*/y;
const reNum = /(?:0[xX][\da-fA-F]+|\d+(?:\.\d+)?)/y;
let hits = 0;
for (let r = 0; r < 4000; r++) {
  const pos = (r * 7) % 40;
  reComment.lastIndex = pos;
  if (reComment.exec(src)) hits++;
  reLine.lastIndex = pos;
  if (reLine.exec(src)) hits++;
  reNum.lastIndex = pos;
  if (reNum.exec(src)) hits++;
}
console.log("sticky_regex", src.length, hits);
