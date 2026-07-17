// Empty v-mode class sets remain always-failing atoms when quantified.
// Spec: ECMAScript 2026, sec-compileatom,
// sec-runtime-semantics-repeatmatcher-abstract-operation

function sameMatch(regexp, input, expected, message) {
  var match = regexp.exec(input);
  if (match === null || match[0] !== expected) {
    throw new Test262Error(
      message + ': expected ' + expected + ', got ' + match
    );
  }
}

function noMatch(regexp, input, message) {
  var match = regexp.exec(input);
  if (match !== null) {
    throw new Test262Error(message + ': expected null, got ' + match);
  }
}

sameMatch(/[\d&&\D]*/v, '', '', 'zero-or-more empty intersection');
sameMatch(/[\w--\w]*/v, '', '', 'zero-or-more empty subtraction');
sameMatch(/[[a]--[a]]?/v, '', '', 'optional nested empty subtraction');
sameMatch(/[\d&&\D]{0,2}/v, '', '', 'bounded empty intersection');
sameMatch(/x[\d&&\D]*y/v, 'xy', 'xy', 'empty intersection in sequence');

noMatch(/[\d&&\D]/v, '', 'bare empty intersection');
noMatch(/[\d&&\D]+/v, '', 'one-or-more empty intersection');
noMatch(/[\d&&\D]{1,2}/v, '', 'positive bounded empty intersection');
