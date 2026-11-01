// Copyright (C) 2026 the JSSE authors. All rights reserved.
/*---
esid: sec-object-initializer-static-semantics-early-errors
description: CoverInitializedName in a spread element of an array literal is a SyntaxError
info: |
  ObjectLiteral : { PropertyDefinitionList }
  PropertyDefinition : CoverInitializedName
  CoverInitializedName : IdentifierReference Initializer

  It is a Syntax Error if any source text is matched by this production
  (13.2.5.1): a CoverInitializedName is only valid where the enclosing
  literal is reinterpreted as an AssignmentPattern, ArrowParameters or a
  for-in/of head.
negative:
  phase: parse
  type: SyntaxError
---*/

$DONOTEVALUATE();

var x = [...{ a = 1 }];
