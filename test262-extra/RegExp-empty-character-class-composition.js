// Empty character classes remain always-failing atoms when composed.
// Spec: ECMAScript 2026, sec-compileatom

function sameValue(actual, expected, message) {
  if (actual !== expected) {
    throw new Test262Error(message + ': expected ' + expected + ', got ' + actual);
  }
}

sameValue(/[]/.test(''), false, 'standalone empty class');
sameValue(/a|[]/.test('a'), true, 'empty class in an alternative');
sameValue(/(?:a|[])*/.exec('aaa')[0], 'aaa', 'quantified alternative');
sameValue(/(?:[^x]|[])*/.exec('abc')[0], 'abc', 'class alternative');
sameValue(
  /\(\*(?:[^(*]|\((?!\*)|\*(?!\))|[])*\*\)/.exec('(* comment *)')[0],
  '(* comment *)',
  'bounded-recursion sentinel'
);
