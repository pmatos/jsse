// Copyright (C) 2026 the JSSE authors. All rights reserved.
/*---
esid: sec-html-like-comments
description: a Module may not begin with an HTML-like close comment after a multi-line comment with a line terminator
info: |
  Comment ::
    MultiLineComment
    SingleLineComment
    SingleLineHTMLOpenComment
    SingleLineHTMLCloseComment
    SingleLineDelimitedComment

  The HTML-like comments of B.1.1 are available in Script code only, so
  a Module that starts with one does not parse.
flags: [module]
negative:
  phase: parse
  type: SyntaxError
---*/

/*
*/ --> not a comment in a module
$DONOTEVALUATE();
