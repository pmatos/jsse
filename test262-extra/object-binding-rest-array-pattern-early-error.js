// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.
/*---
esid: sec-destructuring-binding-patterns
description: the rest property of an object binding pattern must be an identifier, not an array pattern
info: |
  BindingRestProperty[Yield, Await] :
    ... BindingIdentifier[?Yield, ?Await]

  The rest property of an ObjectBindingPattern is a BindingIdentifier, not a
  nested binding pattern.
negative:
  phase: parse
  type: SyntaxError
---*/

$DONOTEVALUATE();

let {...[a]} = {};
