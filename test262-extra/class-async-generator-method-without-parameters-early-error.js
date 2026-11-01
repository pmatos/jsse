// Copyright (C) 2026 the JSSE authors. All rights reserved.
/*---
esid: sec-class-definitions
description: an async generator class element without parameters and a body is a SyntaxError
info: |
  ClassElement : MethodDefinition
  MethodDefinition :
    ClassElementName ( UniqueFormalParameters ) { FunctionBody }
    AsyncMethod
    GeneratorMethod
    AsyncGeneratorMethod
  AsyncMethod : async [no LineTerminator here] ClassElementName ( UniqueFormalParameters ) { AsyncFunctionBody }
  GeneratorMethod : * ClassElementName ( UniqueFormalParameters ) { GeneratorBody }

  `async` and `*` only introduce a method, which needs parameters and a body.
negative:
  phase: parse
  type: SyntaxError
---*/

$DONOTEVALUATE();

var C = class { async *x };
