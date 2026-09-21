// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-for-in-and-for-of-statements
description: for-await-of is not valid in a synchronous function nested in a module
info: |
  The `for await` grammar alternatives are parameterized by [Await] and are not
  available in the body of a synchronous function, even when that function is
  declared in a module.
negative:
  phase: parse
  type: SyntaxError
flags: [module]
features: [async-iteration]
---*/

function f() {
  for await (const value of []) {}
}
