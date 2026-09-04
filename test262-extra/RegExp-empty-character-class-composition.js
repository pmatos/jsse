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

// An empty class is an always-failing but *consuming* atom. Under a quantifier
// that permits zero repetitions it must still yield an empty match at index 0,
// not fail. Spec: ECMAScript 2026, sec-quantifier (RepeatMatcher with min 0)
// applied to the CharacterClass matcher of an empty CharacterClass.
sameValue(/[]*/.exec('abc')[0], '', 'quantified-star empty match');
sameValue(/[]*/.exec('abc').index, 0, 'quantified-star match index');
sameValue(/[]?/.exec('abc')[0], '', 'optional empty match');
sameValue(/[]{0,1}/.exec('abc')[0], '', 'bounded {0,1} empty match');
sameValue(/[]{0,3}/.exec('abc')[0], '', 'bounded {0,3} empty match');
sameValue(/a[]*b/.exec('ab')[0], 'ab', 'empty class between literals');

// Quantifiers that require at least one repetition still fail, as does the
// bare atom.
sameValue(/[]/.exec('abc'), null, 'bare empty class never matches');
sameValue(/[]+/.exec('abc'), null, 'one-or-more empty class fails');
sameValue(/[]{1,3}/.exec('abc'), null, 'bounded {1,3} empty class fails');

// The same holds under the u and v flags, where `[]` is likewise a valid
// empty CharacterClass.
sameValue(/[]*/u.exec('abc')[0], '', 'u-flag quantified empty match');
sameValue(/[]/u.exec('abc'), null, 'u-flag bare empty class never matches');
sameValue(/[]*/v.exec('abc')[0], '', 'v-flag quantified empty match');
sameValue(/[]/v.exec('abc'), null, 'v-flag bare empty class never matches');
