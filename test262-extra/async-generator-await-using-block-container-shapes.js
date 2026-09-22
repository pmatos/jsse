// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-block-runtime-semantics-evaluation
description: >
  `await using` scopes in an async generator dispose at their own exit,
  exactly once, in the containers a lowered block can sit in: an `if`
  consequent, a catch or finally clause, a switch case, a labelled block, and
  next to a function-level resource.
info: |
  Block : { StatementList }

  DisposeResources (proposal-explicit-resource-management,
  sec-disposeresources) runs when the block's evaluation completes.
flags: [async]
includes: [asyncHelpers.js, compareArray.js]
features: [explicit-resource-management, async-iteration]
---*/

asyncTest(async function () {
  var log = [];
  var L = function (entry) { log.push(entry); };
  var D = function (name) {
    return { async [Symbol.asyncDispose]() { L('d-' + name); await null; } };
  };
  async function run(gen) {
    var out = [];
    for await (var v of gen) out.push(v);
    return out.join();
  }
  var check = async function (gen, values, logged, message) {
    log = [];
    assert.sameValue(await run(gen), values, message + ': yielded values');
    assert.compareArray(log, logged, message);
  };

  await check((async function* () {
    if (true) { await using a = D('if'); yield 1; L('in'); }
    L('post');
  })(), '1', ['in', 'd-if', 'post'], 'if consequent');

  await check((async function* () {
    try { throw 'x'; } catch (e) { await using a = D('c'); yield e; }
    L('post');
  })(), 'x', ['d-c', 'post'], 'catch clause');

  await check((async function* () {
    try { yield 1; } finally { await using a = D('fin'); yield 2; L('fin-end'); }
    L('post');
  })(), '1,2', ['fin-end', 'd-fin', 'post'], 'finally clause');

  await check((async function* () {
    switch (1) { case 1: { await using a = D('sw'); yield 's'; } }
    L('post');
  })(), 's', ['d-sw', 'post'], 'switch case');

  await check((async function* () {
    lbl: { await using a = D('o'); { await using b = D('i'); yield 1; break lbl; } L('skipped'); }
    L('post');
  })(), '1', ['d-i', 'd-o', 'post'], 'labelled break out of two scopes');

  await check((async function* () {
    try { { await using a = D('n'); yield 1; return 9; } } finally { L('fin'); }
  })(), '1', ['d-n', 'fin'], 'return through a scope and a finally');

  await check((async function* () {
    await using f = D('fn');
    { await using a = D('b1'); await using b = D('b2'); yield 1; }
    yield 2;
  })(), '1,2', ['d-b2', 'd-b1', 'd-fn'], 'block resources dispose before the function-level one');

  await check((async function* () {
    var i = 0;
    while (i < 3) { i++; { await using a = D('w' + i); if (i === 2) continue; yield i; } }
  })(), '1,3', ['d-w1', 'd-w2', 'd-w3'], 'continue in a while loop');

  await check((async function* () {
    { await using a = D('ys'); yield* [1, 2]; L('after-ys'); }
  })(), '1,2', ['after-ys', 'd-ys'], 'yield* inside the scope');

  await check((async function* () {
    yield 0;
    { await using a = D('nb'); L('body'); }
    yield 1;
  })(), '0,1', ['body', 'd-nb'], 'a yield-free block between yields');

  log = [];
  var it = (async function* () {
    try {
      { await using a = { async [Symbol.asyncDispose]() { throw 'DE'; } }; throw 'orig'; }
    } catch (e) {
      L('caught:' + e.constructor.name + ':' + e.error + ':' + e.suppressed);
    }
    yield 'ok';
  })();
  await it.next();
  assert.compareArray(log, ['caught:SuppressedError:DE:orig'], 'a disposer error chains onto the in-flight throw');
});
