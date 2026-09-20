/*---
description: >
  Top-level await inside a for-in body suspends module evaluation without
  dropping the loop.
esid: sec-runtime-semantics-forinofloopevaluation
info: |
  ForIn/OfBodyEvaluation steps the enumerator and evaluates the body; Await at
  module top level suspends the module's async evaluation and resumes the loop.
flags: [module]
includes: [compareArray.js]
features: [top-level-await]
---*/

const seen = [];
for (const k in { a: 1, b: 2 }) {
  await 0;
  seen.push(k);
}
for (var v in await Promise.resolve({ c: 3 })) seen.push(v);

assert.compareArray(seen, ['a', 'b', 'c'], 'top-level await inside for-in');
