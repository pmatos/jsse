/*---
description: >
  Iterator.prototype.join places separators between every pair of iterator
  values, including empty and nullish values, and coerces the separator before
  looking up next even when the iterator is empty.
esid: sec-iterator.prototype.join
features: [Iterator.prototype.join]
---*/

assert.sameValue(['', 'x'].values().join('-'), '-x');
assert.sameValue([null, 'x'].values().join('-'), '-x');

var effects = [];
var separator = {
  toString: function () {
    effects.push('toString');
    return '-';
  },
};
var iterator = {
  get next() {
    effects.push('get next');
    return function () {
      return { done: true };
    };
  },
};

assert.sameValue(Iterator.prototype.join.call(iterator, separator), '');
assert.compareArray(effects, ['toString', 'get next']);

var loneSurrogate = '\uD800';
assert.sameValue([loneSurrogate].values().join(), loneSurrogate);
assert.sameValue(
  ['a', 'b'].values().join({
    toString: function () {
      return loneSurrogate;
    },
  }),
  'a' + loneSurrogate + 'b'
);
