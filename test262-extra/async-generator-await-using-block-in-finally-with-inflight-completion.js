// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-asyncgeneratorstart
description: >
  An `await using` block inside a `finally` body that is running on behalf of
  an in-flight throw or return completion finishes normally: the rest of the
  `finally` body still runs, and only then does the in-flight completion
  resume, whether it came from `.return()`, `.throw()`, a `throw` statement or
  a `return` statement in the `try` block.
info: |
  TryStatement : try Block Finally

  The completion of the Finally block replaces the Block's completion only if
  it is abrupt; a Block statement that finishes normally inside the Finally
  does not disturb the completion the Finally is running for (sec-try-statement
  runtime semantics). Leaving the `await using` block runs its
  DisposeResources and Awaits (proposal-explicit-resource-management,
  sec-disposeresources).
flags: [async]
includes: [asyncHelpers.js, compareArray.js]
features: [explicit-resource-management, async-iteration]
---*/

function makeGen(L, tryBody) {
  return (async function* () {
    try {
      tryBody();
      yield 1;
    } finally {
      {
        await using x = {
          async [Symbol.asyncDispose]() { L('disp'); }
        };
        L('in-block');
      }
      L('rest-of-finally');
    }
  })();
}

async function drive(tryBody, act) {
  var log = [];
  var L = function (entry) { log.push(entry); };
  var it = makeGen(L, tryBody);
  var first = await it.next();
  if (first.value !== 1) {
    log.push('first:' + first.value);
  }
  try {
    var r = await act(it);
    log.push('r:' + r.value + ':' + r.done);
  } catch (e) {
    log.push('rej:' + e);
  }
  return log;
}

asyncTest(async function () {
  var log = await drive(function () {}, function (it) { return it.return('R'); });
  assert.compareArray(
    log,
    ['in-block', 'disp', 'rest-of-finally', 'r:R:true'],
    '.return() at a yield resumes its return after the finally body'
  );

  log = await drive(function () {}, function (it) { return it.throw('T'); });
  assert.compareArray(
    log,
    ['in-block', 'disp', 'rest-of-finally', 'rej:T'],
    '.throw() at a yield resumes its throw after the finally body'
  );
});

asyncTest(async function () {
  var log = [];
  var L = function (entry) { log.push(entry); };
  var it = (async function* () {
    try {
      throw 'E';
    } finally {
      {
        await using x = {
          async [Symbol.asyncDispose]() { L('disp'); }
        };
        L('in-block');
      }
      L('rest-of-finally');
    }
  })();
  try { await it.next(); log.push('resolved'); }
  catch (e) { log.push('rej:' + e); }
  assert.compareArray(
    log,
    ['in-block', 'disp', 'rest-of-finally', 'rej:E'],
    'a throw statement in the try block resumes after the finally body'
  );
});

asyncTest(async function () {
  var log = [];
  var L = function (entry) { log.push(entry); };
  var it = (async function* () {
    try {
      return 'X';
    } finally {
      {
        await using x = {
          async [Symbol.asyncDispose]() { L('disp'); }
        };
        L('in-block');
      }
      L('rest-of-finally');
    }
  })();
  var r = await it.next();
  log.push('r:' + r.value + ':' + r.done);
  assert.compareArray(
    log,
    ['in-block', 'disp', 'rest-of-finally', 'r:X:true'],
    'a return statement in the try block resumes after the finally body'
  );
});
