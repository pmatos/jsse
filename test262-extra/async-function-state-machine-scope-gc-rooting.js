// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-async-function-definitions-runtime-semantics-evaluation
description: >
  Block scopes an async function enters during one run stay reachable while a
  call made by that run collects garbage, even though the function has not
  suspended since entering them. The `for await` keeps the body on the state
  machine, whose lowered scopes are saved only at the next suspension.
flags: [async]
includes: [compareArray.js]
features: [async-iteration, host-gc-required]
---*/

function collect() {
  var junk = [];
  for (var i = 0; i < 200; i++) junk.push({ i: i, a: [i] });
  $262.gc();
  return 1;
}

async function nestedScopes() {
  var out = [];
  for (const type of ["text", "bytes"]) {
    const messages = [{ kind: type }];
    for (const name of ["a", "b"]) {
      const result = await collect();
      messages.push({ name: name, result: result });
    }
    out.push(type + ":" + messages.map(function (m) { return m.kind || m.name + m.result; }).join(","));
  }
  for await (const unused of []) {
    out.push("unreachable");
  }
  return out.join("|");
}

nestedScopes().then(function (value) {
  assert.sameValue(value, "text:text,a1,b1|bytes:bytes,a1,b1");
}).then($DONE, $DONE);
