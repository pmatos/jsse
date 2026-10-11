// Copyright (C) 2026 the JSSE project authors. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.
/*---
esid: sec-template-literals-runtime-semantics-evaluation
description: >
  A template literal substitution is converted with ToString, which keeps
  lone surrogates, including those returned from an object's toString.
  Expected values are cross-checked with Node.
info: |
  13.2.8.6 Runtime Semantics: Evaluation
    TemplateMiddleList : TemplateMiddle Expression
      ...
      3. Let middle be ? ToString(sub).
---*/

var lone = "\ud800";
assert.sameValue(`${lone}`, "\ud800", "string substitution");
assert.sameValue(`${lone}`.charCodeAt(0), 0xd800, "string substitution code unit");
var obj = { toString: function () { return "\udc00"; } };
assert.sameValue(`x${obj}y`, "x\udc00y", "object substitution");
assert.sameValue(`\ud83d${"\ude00"}`, "\ud83d\ude00", "halves join into a pair");
