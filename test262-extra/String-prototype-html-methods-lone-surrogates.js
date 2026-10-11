// Copyright (C) 2026 the JSSE project authors. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.
/*---
esid: sec-createhtml
description: >
  The Annex B HTML methods keep lone surrogates in both the receiver and the
  attribute value. Expected values are cross-checked with Node.
info: |
  B.2.2.2.1 CreateHTML ( string, tag, attribute, value )
    2. Let S be ? ToString(str).
    ...
    5. If attribute is not the empty String, then
      a. Let V be ? ToString(value).
      b. Let escapedV be the String value that is the same as V except that
         each occurrence of the code unit 0x0022 (QUOTATION MARK) in V has
         been replaced with the six code unit sequence "&quot;".
---*/

assert.sameValue("\ud800".big(), "<big>\ud800</big>", "big");
assert.sameValue("\udc00".anchor("\ud800\""), "<a name=\"\ud800&quot;\">\udc00</a>", "anchor");
assert.sameValue("x".link("\udc00"), "<a href=\"\udc00\">x</a>", "link");
