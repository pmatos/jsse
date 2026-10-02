/*---
description: >
  A `for` statement's own LexicalDeclaration head, whose sole declarator's
  initializer is itself the suspension point (`for (const i = yield; ...;
  ...)`), must bind the declared name in the per-iteration Environment
  created for the head -- not in a function-level temporary variable that
  happens to share a name with an outer `var` of the same function. Only
  the identity-collision consequence is exercised here (the block case
  covers both TDZ and identity); this covers the distinct
  `initial_lexical_bindings` pre-declare at the `for`-head call site.
esid: sec-createperiterationenvironment
info: |
  CreatePerIterationEnvironment ( perIterationBindings )

  ForStatement : for ( LexicalDeclaration Expression ; Expression ) Statement

  1. Let oldEnv be the running execution context's LexicalEnvironment.
  2. Let loopEnv be NewDeclarativeEnvironment(oldEnv).
  3. ...
  4. Perform ForDeclarationBindingInstantiation of LexicalDeclaration with
     argument loopEnv.

  The head's own bound names get their own binding in loopEnv, a fresh
  Environment Record distinct from any outer `var` of the same name --
  independent of whether the LexicalDeclaration's own initializer is a
  suspension point.
features: [generators]
---*/

function* g() {
  var i = 1;
  for (const i = yield; false; ) {
    // never runs: the head's own binding is only exercised by its
    // initializer and the (always-false) test.
  }
  return i;
}

var it = g();
it.next();
var result = it.next(999);
assert.sameValue(result.value, 1, 'the outer `var i` is untouched by the for-head\'s own binding');
assert.sameValue(result.done, true, 'the generator completes normally');
