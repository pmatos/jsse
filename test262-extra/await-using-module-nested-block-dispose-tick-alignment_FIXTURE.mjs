// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

const L = (entry) => globalThis.moduleAwaitUsingLog.push(entry);

Promise.resolve()
  .then(() => L('w1'))
  .then(() => L('w2'))
  .then(() => L('w3'))
  .then(() => L('w4'));

try {
  {
    await using a = {
      async [Symbol.asyncDispose]() {
        L('disposer');
      }
    };
    L('try-body');
    throw new Error('boom');
  }
} catch (e) {
  L('caught-' + e.message);
}

let i = 0;
while (i < 2) {
  i++;
  {
    await using b = null;
    L('loop' + i);
  }
}
L('end');
