// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-disposablestack.prototype.dispose
description: >
  Resources registered on a DisposableStack / AsyncDisposableStack via
  use(), adopt(), and defer() stay reachable across a garbage collection,
  both before dispose() runs and while it is partway through disposing.
info: |
  DisposableStack.prototype.use ( value )
  DisposableStack.prototype.adopt ( value, onDispose )
  DisposableStack.prototype.defer ( onDispose )
  DisposableStack.prototype.dispose ( )
  AsyncDisposableStack.prototype.disposeAsync ( )

  DisposeResources ( disposeCapability, completion )

  3. For each element resource of disposeCapability.[[DisposableResourceStack]], in reverse list order, do
     [...]
     e. If method is not undefined, then
        i. Let result be Completion(Call(method, value)).
        [...]
        iii. If result is a throw completion, then
             1. If completion is a throw completion, then
                [...] Set completion to ThrowCompletion(error) (a SuppressedError)
             2. Else, set completion to result.
flags: [async]
includes: [asyncHelpers.js]
features: [explicit-resource-management, host-gc-required]
---*/

// --- pre-disposal window: use() and defer(), sync stack ---
(function () {
  var log = [];
  var stack = new DisposableStack();
  stack.use({ [Symbol.dispose]() { log.push('use'); } });
  stack.defer(function () { log.push('defer'); });
  $262.gc();
  stack.dispose();
  assert.sameValue(log.join(), 'defer,use', 'sync: use()/defer() resources survive a gc before dispose()');
})();

// --- pre-disposal window: adopt(), sync stack ---
(function () {
  var log = [];
  var stack = new DisposableStack();
  stack.adopt({ id: 'handle' }, function (h) { log.push('adopted-' + h.id); });
  $262.gc();
  stack.dispose();
  assert.sameValue(log.join(), 'adopted-handle', 'sync: adopt() wrapper captures survive a gc before dispose()');
})();

// --- mid-disposal window: resource survival while an earlier-processed disposer runs (gap 2a) ---
(function () {
  var log = [];
  var stack = new DisposableStack();
  // Registered first, so disposed last; the sole reference to this callback
  // lives in the (by then taken) Rust stack vector.
  stack.defer(function () { log.push('first-registered'); });
  // Registered second, so disposed first; triggers a gc while the
  // first-registered callback is still waiting to run.
  stack.defer(function () {
    $262.gc();
    log.push('second-registered');
  });
  stack.dispose();
  assert.sameValue(
    log.join(),
    'second-registered,first-registered',
    'sync: a resource registered before the gc-triggering one survives disposal'
  );
})();

// --- mid-disposal window: SuppressedError accumulation across a gc (gap 2b) ---
(function () {
  var stack = new DisposableStack();
  // Registered first, so disposed last.
  stack.defer(function () { throw new RangeError('first'); });
  // Registered second, so disposed second-to-last; the gc fires while
  // `current_error` already holds the "second" error.
  stack.defer(function () { $262.gc(); });
  // Registered last, so disposed first.
  stack.defer(function () { throw new RangeError('second'); });

  var caught;
  try {
    stack.dispose();
  } catch (e) {
    caught = e;
  }
  assert.notSameValue(caught, undefined, 'sync: dispose() should throw a SuppressedError');
  assert.sameValue(caught.constructor.name, 'SuppressedError', 'sync: error survives gc as a SuppressedError');
  assert.sameValue(caught.error.message, 'first', 'sync: outer error survives gc');
  assert.sameValue(caught.suppressed.message, 'second', 'sync: suppressed error survives gc');
})();

asyncTest(async function () {
  // --- pre-disposal window: use()/defer(), async stack ---
  {
    var log = [];
    var stack = new AsyncDisposableStack();
    stack.use({ [Symbol.asyncDispose]() { log.push('use'); } });
    stack.defer(async function () { log.push('defer'); });
    $262.gc();
    await stack.disposeAsync();
    assert.sameValue(log.join(), 'defer,use', 'async: use()/defer() resources survive a gc before disposeAsync()');
  }

  // --- pre-disposal window: adopt(), async stack ---
  {
    var log = [];
    var stack = new AsyncDisposableStack();
    stack.adopt({ id: 'handle' }, function (h) { log.push('adopted-' + h.id); });
    $262.gc();
    await stack.disposeAsync();
    assert.sameValue(log.join(), 'adopted-handle', 'async: adopt() wrapper captures survive a gc before disposeAsync()');
  }

  // --- mid-disposal window: resource survival (gap 2a confirmation, async path) ---
  {
    var log = [];
    var stack = new AsyncDisposableStack();
    stack.defer(async function () { log.push('first-registered'); });
    stack.defer(async function () {
      $262.gc();
      log.push('second-registered');
    });
    await stack.disposeAsync();
    assert.sameValue(
      log.join(),
      'second-registered,first-registered',
      'async: a resource registered before the gc-triggering one survives disposeAsync()'
    );
  }

  // --- mid-disposal window: SuppressedError accumulation (gap 2b confirmation, async path) ---
  {
    var stack = new AsyncDisposableStack();
    stack.defer(async function () { throw new RangeError('first'); });
    stack.defer(async function () { $262.gc(); });
    stack.defer(async function () { throw new RangeError('second'); });

    var caught;
    try {
      await stack.disposeAsync();
    } catch (e) {
      caught = e;
    }
    assert.notSameValue(caught, undefined, 'async: disposeAsync() should throw a SuppressedError');
    assert.sameValue(caught.constructor.name, 'SuppressedError', 'async: error survives gc as a SuppressedError');
    assert.sameValue(caught.error.message, 'first', 'async: outer error survives gc');
    assert.sameValue(caught.suppressed.message, 'second', 'async: suppressed error survives gc');
  }
});
