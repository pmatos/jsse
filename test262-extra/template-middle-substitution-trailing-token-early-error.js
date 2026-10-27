// Copyright (C) 2026 the JSSE authors. All rights reserved.
/*---
esid: sec-template-literals
description: a token between a later template substitution and its closing brace is a SyntaxError
info: |
  TemplateLiteral[Yield, Await, Tagged] :
    NoSubstitutionTemplate
    SubstitutionTemplate[?Yield, ?Await, ?Tagged]

  SubstitutionTemplate[Yield, Await, Tagged] :
    TemplateHead Expression[+In, ?Yield, ?Await] TemplateSpans[?Yield, ?Await, ?Tagged]

  An Expression must be followed directly by the } that begins TemplateMiddle or TemplateTail.
negative:
  phase: parse
  type: SyntaxError
---*/

$DONOTEVALUATE();

var x = `a${1}b${2 3}`;
