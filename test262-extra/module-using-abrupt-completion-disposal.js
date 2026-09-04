/*---
description: Module-level using resources are disposed in LIFO order during abrupt completion
esid: sec-source-text-module-record-execute-module
info: |
  ExecuteModule passes the completion of evaluating ECMAScriptCode to
  DisposeResources for the module environment. DisposeResources visits the
  disposable resource stack in reverse list order and combines a disposal
  error with an existing throw in a SuppressedError.
flags: [module, async]
includes: [compareArray.js]
features: [dynamic-import, explicit-resource-management]
---*/

globalThis.moduleUsingDisposalLog = [];
globalThis.moduleUsingBodyError = {};
globalThis.moduleUsingDisposeError = {};

import('./module-using-abrupt-completion-disposal_FIXTURE.mjs').then(
  function () {
    $DONE(new Test262Error('the fixture module should reject'));
  },
  function (error) {
    try {
      assert(error instanceof SuppressedError, 'disposal should suppress the module body error');
      assert.sameValue(
        error.error,
        globalThis.moduleUsingDisposeError,
        'the disposal error should be primary'
      );
      assert.sameValue(
        error.suppressed,
        globalThis.moduleUsingBodyError,
        'the module body error should be suppressed'
      );
      assert.compareArray(
        globalThis.moduleUsingDisposalLog,
        ['second', 'first'],
        'all resources should be disposed in reverse declaration order'
      );
      $DONE();
    } catch (assertionError) {
      $DONE(assertionError);
    }
  }
);
