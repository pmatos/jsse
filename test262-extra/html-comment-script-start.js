// Copyright (C) 2026 the JSSE authors. All rights reserved.
/*---
esid: sec-html-like-comments
description: Script code may begin with an HTML-like comment
info: |
  Comment ::
    SingleLineHTMLOpenComment
    SingleLineHTMLCloseComment

  The HTML-like comments of B.1.1 are available in Script code, including
  before its first token; Module code rejects them.
---*/
<!-- the first token of this Script follows an HTML-like comment
var viaOpen = 1;
/* spans
   lines */ --> and so does a close comment
var viaClose = 2;
assert.sameValue(viaOpen + viaClose, 3);
