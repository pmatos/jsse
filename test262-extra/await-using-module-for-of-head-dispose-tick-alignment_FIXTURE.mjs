// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

const L = (entry) => globalThis.moduleForOfHeadLog.push(entry);

Promise.resolve()
  .then(() => L('w1'))
  .then(() => L('w2'))
  .then(() => L('w3'))
  .then(() => L('w4'));

for (await using a of [
  { async [Symbol.asyncDispose]() { L('disp'); } },
  { async [Symbol.asyncDispose]() { L('disp2'); } },
]) {
  L('body');
}
L('end');
