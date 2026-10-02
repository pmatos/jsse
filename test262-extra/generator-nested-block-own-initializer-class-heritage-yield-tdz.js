/*---
description: >
  A block-scoped `const` declarator whose own initializer is an anonymous
  class expression with a suspension in its heritage
  (`const x = class extends (yield) {};`) must satisfy three requirements
  at once: (1) the declared name gets a real, TDZ'd binding in the
  block's own Environment Record (so an earlier closure reading the
  shadowed name throws a TDZ ReferenceError), (2) that binding is a
  distinct storage location from an outer `var` of the same name (so
  resuming the generator does not clobber the outer `var`), and (3) the
  class still gets NamedEvaluation'd with the declared name, exactly as
  it would without any suspension in its heritage.
esid: sec-blockdeclarationinstantiation
info: |
  BlockDeclarationInstantiation ( code, env ) pre-declares every lexically
  scoped declaration of a Block, uninitialized, in the block's own
  Environment Record before any statement of the block runs (see
  `generator-nested-block-own-initializer-yield-tdz.js` for the general
  case). Independently, `sec-runtime-semantics-classdefinitionevaluation`'s
  caller -- `LexicalBinding : BindingIdentifier Initializer` -- performs
  NamedEvaluation when Initializer is an anonymous class expression,
  regardless of whether evaluating that class's heritage happens to
  involve a suspension partway through.
features: [generators, class]
---*/

class Base {}

var outerX = 'outer';
function* tdz() {
  var x = outerX;
  {
    const y = (function () { return x; })();
    const x = class extends (yield 'h') {};
    return [y, x.name];
  }
}

var itTdz = tdz();
assert.throws(
  ReferenceError,
  function () { itTdz.next(); },
  'reading the shadowed name before the block\'s own `const x = class extends ' +
    '(yield) {}` declaration runs throws a TDZ ReferenceError'
);

function* collide() {
  var x = 1;
  {
    const x = class extends (yield 'h') {};
  }
  return x;
}

var itCollide = collide();
itCollide.next();
var collideResult = itCollide.next(Base);
assert.sameValue(collideResult.value, 1, 'the outer `var x` is untouched by the resumed class');
assert.sameValue(collideResult.done, true, 'the generator completes normally');

function* namedValue() {
  {
    const x = class extends (yield 'h') {};
    return x.name;
  }
}

var itNamed = namedValue();
itNamed.next();
assert.sameValue(
  itNamed.next(Base).value,
  'x',
  'the class is still named after the declared identifier despite the heritage suspension'
);
