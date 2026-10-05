// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-functiondeclarationinstantiation
description: >
  A sloppy-mode `function` declaration nested inside a block/loop body of a
  (synchronous) generator is hoisted to the generator function's own var
  scope (Annex B.3.3), even once that body is lowered to a state machine by
  a `yield`.
info: |
  FunctionDeclarationInstantiation ( func, argumentsList )

  [...]
  <web-compat insertion point>: For each FunctionDeclaration f that is
  directly contained in the StatementList of a Block, CaseClause, or
  DefaultClause, in source text order, do
    1. Let F be StringValue of the BindingIdentifier of f.
    2. If replacing the FunctionDeclaration f with a VariableStatement that
       has F as a BindingIdentifier would not produce any Early Errors for
       func and F is not an element of parameterNames, then
       a. ...
       b. If initializedBindings does not already have F, then
          i. Perform ! varEnvRec.CreateMutableBinding(F, false).
          ii. Perform varEnvRec.InitializeBinding(F, undefined).
          iii. Append F to initializedBindings.

  EvaluateGeneratorBody
    1. Perform ? FunctionDeclarationInstantiation(functionObject, argumentsList).

  Step 1 runs synchronously, at `GeneratorStart`, before the state machine
  driving the generator is ever resumed — so the var binding for a
  block/loop-nested function declaration must already exist the moment the
  generator is called, not merely once the block containing it happens to
  execute (issue #842).
flags: [noStrict]
---*/

(function () {
  function* main() {
    // Bare block.
    {
      function g() {}
      yield 0;
    }
    assert.sameValue(typeof g, 'function', 'function declared in a bare block');

    // `while` body.
    let i = 0;
    while (i < 1) {
      function h() {}
      yield 0;
      i++;
    }
    assert.sameValue(typeof h, 'function', 'function declared in a while body');

    // `for (var ...)` body.
    for (var fi = 0; fi < 1; fi++) {
      function j() {}
      yield 0;
    }
    assert.sameValue(typeof j, 'function', 'function declared in a for (var ...) body');

    // Init-order: the binding must exist (as `undefined`) at generator
    // entry, before the block that textually declares it ever runs.
    let beforeK = k;
    {
      function k() {}
      yield 0;
    }
    assert.sameValue(beforeK, undefined, 'k is bound to undefined before its declaring block runs');
    assert.sameValue(typeof k, 'function', 'k is hoisted after its declaring block runs');

    // Mixed inline + split: `a`'s block has no suspension point before it,
    // so it stays inlined in the depth-0 fragment; `b`'s block is split off
    // into its own state. Both names must still end up on the var scope.
    {
      function a() {}
    }
    {
      function b() {}
      yield 0;
    }
    assert.sameValue(typeof a, 'function', 'function in an unsplit (inline) block sibling');
    assert.sameValue(typeof b, 'function', 'function in a split (lowered) block sibling');
  }

  let it = main();
  while (!it.next().done) {}
})();

(function () {
  // Parameter shadow (simple parameter list): the declaration must be
  // skipped because `parameterNames` already contains `shadowed`, so the
  // parameter's own value must survive untouched.
  function* paramShadow(shadowed) {
    {
      function shadowed() {}
      yield 0;
    }
    return shadowed;
  }
  let it = paramShadow('param-value');
  let r;
  do {
    r = it.next();
  } while (!r.done);
  assert.sameValue(
    r.value,
    'param-value',
    "a block function sharing a parameter's name does not clobber the parameter"
  );
})();

(function () {
  // Lexical conflict: a top-level `let` of the same name blocks the Annex B
  // binding for a same-named nested function declaration entirely.
  function* lexConflict() {
    let lexConflict = 'lex-value';
    {
      function lexConflict() {}
      yield 0;
    }
    return lexConflict;
  }
  let it = lexConflict();
  let r;
  do {
    r = it.next();
  } while (!r.done);
  assert.sameValue(r.value, 'lex-value', 'a top-level let of the same name blocks Annex B hoisting');
})();

(function () {
  // "arguments": modeled on the (already-passing, tree-walker-only)
  // test262/test/annexB/language/function-code/block-decl-func-skip-arguments.js
  // — a guard confirming the lowered path keeps agreeing with the
  // tree-walker, not a red/green check for this issue.
  function* argsGuard() {
    let before = Object.prototype.toString.call(arguments);
    let r1, r2;
    {
      r1 = arguments();
      function arguments() {}
      r2 = arguments();
      yield 0;
    }
    let after = Object.prototype.toString.call(arguments);
    return [before, r1, r2, after];
  }
  let it = argsGuard();
  let r;
  do {
    r = it.next();
  } while (!r.done);
  assert.sameValue(
    JSON.stringify(r.value),
    JSON.stringify(['[object Arguments]', undefined, undefined, '[object Arguments]']),
    'the outer arguments object is never replaced by the block-scoped function named "arguments"'
  );
})();

(function () {
  // Strict mode control: nothing is hoisted, matching the tree-walker.
  function* strictControl() {
    'use strict';
    {
      function sc() {}
      yield 0;
    }
    return typeof sc;
  }
  let it = strictControl();
  let r;
  do {
    r = it.next();
  } while (!r.done);
  assert.sameValue(r.value, 'undefined', 'strict mode does not hoist block functions');
})();
