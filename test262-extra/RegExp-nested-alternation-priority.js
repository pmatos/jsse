// Nested quantified alternatives preserve ECMAScript match priority.
// Spec: ECMAScript 2026, sec-regexpbuiltinexec, sec-compilesubpattern,
// sec-runtime-semantics-repeatmatcher-abstract-operation

function assertMatch(match, expectedText, expectedIndex, expectedCapture, message) {
  if (match === null) {
    throw new Test262Error(message + ': expected a match, got null');
  }
  if (match[0] !== expectedText) {
    throw new Test262Error(
      message + ': expected ' + expectedText + ', got ' + match[0]
    );
  }
  if (match.index !== expectedIndex) {
    throw new Test262Error(
      message + ': expected index ' + expectedIndex + ', got ' + match.index
    );
  }
  if (expectedCapture !== undefined && match[1] !== expectedCapture) {
    throw new Test262Error(
      message + ': expected capture ' + expectedCapture + ', got ' + match[1]
    );
  }
}

var parentheses =
  /(^|[^^])\((?:[^()]|\((?:[^()]|\((?:[^()])*\))*\))*\)/.exec(
    '((3-(9-2))*4)'
  );
assertMatch(
  parentheses,
  '((3-(9-2))*4)',
  0,
  '',
  'nested parentheses choose the earliest greedy match'
);

var braces = /\{(?:\{[^}]*\}|[^{}])*\}/.exec('{{x} y}');
assertMatch(
  braces,
  '{{x} y}',
  0,
  undefined,
  'nested braces choose the complete greedy match'
);
