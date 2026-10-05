/*---
description: >
  A switch statement's CaseBlock is a single lexical environment spanning
  every case, even when the function's state-machine lowering splits a case
  across an await. A case-local declaration must not clobber an outer
  binding of the same name once the switch exits, the discriminant and case
  tests must evaluate in the environment that existed before the CaseBlock's
  environment was created, and the CaseBlock's one shared environment must
  still let an earlier case's declaration be observed by a later case on
  fallthrough.
esid: sec-switch-statement-runtime-semantics-evaluation
info: |
  SwitchStatement : switch ( Expression ) CaseBlock

  1. Let exprRef be ? Evaluation of Expression.
  2. Let switchValue be ? GetValue(exprRef).
  3. Let oldEnv be the running execution context's LexicalEnvironment.
  4. Let blockEnv be NewDeclarativeEnvironment(oldEnv).
  5. Perform BlockDeclarationInstantiation(CaseBlock, blockEnv).
  6. Set the running execution context's LexicalEnvironment to blockEnv.
  7. Let R be Completion(CaseBlockEvaluation of CaseBlock with argument switchValue).
  8. Set the running execution context's LexicalEnvironment to oldEnv.
  9. Return ? R.

  NOTE: No matter how control leaves the SwitchStatement the
  LexicalEnvironment is always restored to its former state.
flags: [async]
includes: [compareArray.js]
features: [async-functions]
---*/

async function caseLocalLetRestoredAfterSwitch() {
  var y = 'outer';
  var out = [];
  switch (1) {
    case 1:
      let y = 'inner';
      await 0;
      out.push(y);
      break;
  }
  out.push(y);
  return out;
}

async function caseLocalLetRestoredAfterSwitchFallsThrough() {
  var y = 'outer';
  var out = [];
  switch (1) {
    case 1:
      let y = 'inner';
      await 0;
      out.push(y);
  }
  out.push(y);
  return out;
}

async function discriminantEvaluatesInOuterEnvironment() {
  let x = 'outer';
  let out = [];
  switch (x) {
    case 'outer':
      let x = 'inner';
      await 0;
      out.push(x);
      break;
  }
  return out;
}

async function caseBlockScopeIsSharedAcrossFallthrough() {
  let out = [];
  switch (1) {
    case 1:
      let shared = 'from-case-1';
      await 0;
    case 2:
      out.push(shared);
      break;
  }
  return out;
}

caseLocalLetRestoredAfterSwitch()
  .then(function (out) {
    assert.compareArray(
      out,
      ['inner', 'outer'],
      'a case-local let must not clobber the outer binding once the switch (with a break) exits'
    );
    return caseLocalLetRestoredAfterSwitchFallsThrough();
  })
  .then(function (out) {
    assert.compareArray(
      out,
      ['inner', 'outer'],
      'a case-local let must not clobber the outer binding once the switch (falling through to the end) exits'
    );
    return discriminantEvaluatesInOuterEnvironment();
  })
  .then(function (out) {
    assert.compareArray(
      out,
      ['inner'],
      'the discriminant must evaluate in the environment before CaseBlock, not throw a TDZ error against the ' +
        'case-local binding of the same name'
    );
    return caseBlockScopeIsSharedAcrossFallthrough();
  })
  .then(function (out) {
    assert.compareArray(
      out,
      ['from-case-1'],
      "the CaseBlock's single shared environment must let a later case observe an earlier case's declaration on fallthrough"
    );
  })
  .then($DONE, $DONE);
