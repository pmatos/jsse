// Measured 2026-10-10, jsse HEAD 2b621c5 vs boa 0.22.0, buildbox3, median of 5 (loadavg 10-12): jsse 3.43 s, boa 1.27 s, ratio 2.70x, jsse --no-bytecode (median of 3) 3.43 s.
// Exercises: a js-tokens style tokenizer: ONE generator that loops over an
// input string, tries a fixed list of sticky regexes at lastIndex, and yields
// one {type, value} object per token, consumed by for-of.
// Models: js-tokens (the whole benchmark has this shape).
// Reduced: 6 sticky regexes instead of 14, ASCII only (no \p{...}).
const Punctuator = /--|\+\+|=>|\.{3}|&&|\|\||[+\-%&|^*<>!=?~,:;[\](){}.\/]=?/y;
const Identifier = /[$_A-Za-z][$_A-Za-z0-9]*/y;
const StringLiteral = /(['"])(?:(?!\1)[^\\\n\r]|\\[^])*\1/y;
const NumericLiteral = /(?:0[xX][\da-fA-F]+|\d+(?:\.\d+)?)/y;
const WhiteSpace = /[ \t]+/y;
const LineTerminator = /\r?\n/y;
function* tokens(input) {
  let lastIndex = 0, match;
  const length = input.length;
  while (lastIndex < length) {
    Punctuator.lastIndex = lastIndex;
    if (match = Punctuator.exec(input)) {
      lastIndex = Punctuator.lastIndex;
      yield { type: "Punctuator", value: match[0] };
      continue;
    }
    Identifier.lastIndex = lastIndex;
    if (match = Identifier.exec(input)) {
      lastIndex = Identifier.lastIndex;
      yield { type: "IdentifierName", value: match[0] };
      continue;
    }
    StringLiteral.lastIndex = lastIndex;
    if (match = StringLiteral.exec(input)) {
      lastIndex = StringLiteral.lastIndex;
      yield { type: "StringLiteral", value: match[0] };
      continue;
    }
    NumericLiteral.lastIndex = lastIndex;
    if (match = NumericLiteral.exec(input)) {
      lastIndex = NumericLiteral.lastIndex;
      yield { type: "NumericLiteral", value: match[0] };
      continue;
    }
    WhiteSpace.lastIndex = lastIndex;
    if (match = WhiteSpace.exec(input)) {
      lastIndex = WhiteSpace.lastIndex;
      yield { type: "WhiteSpace", value: match[0] };
      continue;
    }
    LineTerminator.lastIndex = lastIndex;
    if (match = LineTerminator.exec(input)) {
      lastIndex = LineTerminator.lastIndex;
      yield { type: "LineTerminatorSequence", value: match[0] };
      continue;
    }
    yield { type: "Invalid", value: input[lastIndex++] };
  }
}
let src = "";
for (let i = 0; i < 40; i++) {
  src += "function f" + i + "(a, b) {\n  const s = 'text" + i + "';\n  if (a >= 0x" + i + " && b != 3.5) { return a + b * " + i + "; }\n  return s.length;\n}\n";
}
let n = 0, h = 0;
for (let r = 0; r < 30; r++) {
  for (const t of tokens(src)) { n++; h = (h * 31 + t.value.length) | 0; }
}
console.log("tokenizer", src.length, n, h);
