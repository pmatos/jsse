// Copyright (C) 2026 the JSSE authors. All rights reserved.
/*---
esid: sec-try-statement
description: a try statement with neither a catch nor a finally clause is a SyntaxError
info: |
  TryStatement[Yield, Await, Return] :
    try Block[?Yield, ?Await, ?Return] Catch[?Yield, ?Await, ?Return]
    try Block[?Yield, ?Await, ?Return] Finally[?Yield, ?Await, ?Return]
    try Block[?Yield, ?Await, ?Return] Catch[?Yield, ?Await, ?Return] Finally[?Yield, ?Await, ?Return]
negative:
  phase: parse
  type: SyntaxError
---*/

$DONOTEVALUATE();

try {
}
