// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-exports-static-semantics-early-errors
description: >
  Without a `from` clause, the local name of an export specifier must be a valid
  IdentifierReference, so the literal true is a SyntaxError.
info: |
  ExportDeclaration : export NamedExports ;
    It is a Syntax Error if the StringValue of any ReferencedBindings of
    NamedExports is a ReservedWord.
negative:
  phase: parse
  type: SyntaxError
flags: [module]
---*/

$DONOTEVALUATE();

export { true as t };
