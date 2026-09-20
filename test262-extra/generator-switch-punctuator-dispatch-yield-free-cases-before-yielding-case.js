/*---
description: >
  A tokenizer-style generator dispatches on a punctuator with a `switch` whose
  early clauses (`(`, `)`, `{`) have no `yield` and end in `break`, followed by
  clauses that do yield. The early clauses must not fall through into the `}`
  clause, otherwise the state they track (the last significant token) is
  corrupted and later decisions, such as entering JSX mode after `return (`,
  are taken wrongly. This mirrors the `js-tokens` JetStream workload.
esid: sec-runtime-semantics-caseblockevaluation
info: |
  Runtime Semantics: CaseBlockEvaluation

  A `break` completion in a selected clause is abrupt, so no later clause is
  evaluated (14.12.4).
includes: [compareArray.js]
features: [generators]
---*/

var TokensPrecedingExpression = /^(?:return|\()$/;

function* tokenize(input) {
  var lastSignificantToken = "";
  var nextLastSignificantToken = "";
  var braces = [];
  var postfixIncDec = false;
  var i = 0;

  while (i < input.length) {
    var ch = input[i];
    var isPunct = "(){}<>/".indexOf(ch) !== -1;
    var isWord = /[a-z]/.test(ch);
    var isSpace = ch === " ";

    if (isSpace) {
      i++;
      continue;
    }

    if (isWord) {
      var start = i;
      while (i < input.length && /[a-z]/.test(input[i])) i++;
      var word = input.slice(start, i);
      yield { type: "Word", value: word };
      nextLastSignificantToken = word;
      lastSignificantToken = nextLastSignificantToken;
      continue;
    }

    if (isPunct) {
      nextLastSignificantToken = ch;
      switch (ch) {
        case "(":
          postfixIncDec = false;
          break;

        case ")":
          postfixIncDec = true;
          break;

        case "{":
          braces.push(ch);
          postfixIncDec = false;
          break;

        case "}":
          postfixIncDec = braces.pop() === "{";
          nextLastSignificantToken = "}";
          switch (braces.length) {
            case 0:
              yield { type: "Punctuator", value: ch };
              break;
            default:
              yield { type: "Punctuator", value: ch };
          }
          lastSignificantToken = nextLastSignificantToken;
          i++;
          continue;

        case "<":
          if (TokensPrecedingExpression.test(lastSignificantToken)) {
            yield { type: "JSXPunctuator", value: ch };
            lastSignificantToken = ch;
            i++;
            continue;
          }
          break;

        default:
          break;
      }
      yield { type: "Punctuator", value: ch };
      lastSignificantToken = nextLastSignificantToken;
      i++;
      continue;
    }

    i++;
  }
}

function summarize(input) {
  return Array.from(tokenize(input), function(t) {
    return t.type + ":" + t.value;
  });
}

assert.compareArray(
  summarize("return (<a>)"),
  ["Word:return", "Punctuator:(", "JSXPunctuator:<", "Word:a", "Punctuator:>", "Punctuator:)"],
  "`<` after `return (` starts JSX"
);

assert.compareArray(
  summarize("x < y"),
  ["Word:x", "Punctuator:<", "Word:y"],
  "`<` after an identifier is a plain punctuator"
);

assert.compareArray(
  summarize("(<a>)"),
  ["Punctuator:(", "JSXPunctuator:<", "Word:a", "Punctuator:>", "Punctuator:)"],
  "`<` directly after `(` starts JSX"
);

assert.compareArray(
  summarize("{ } ( <a>"),
  ["Punctuator:{", "Punctuator:}", "Punctuator:(", "JSXPunctuator:<", "Word:a", "Punctuator:>"],
  "`{` and `}` do not disturb the last significant token seen by a later `(`"
);
