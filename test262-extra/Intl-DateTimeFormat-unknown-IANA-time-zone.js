// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-createdatetimeformat
description: Unknown IANA-shaped time zone identifiers are rejected
---*/

assert.throws(RangeError, function () {
  new Intl.DateTimeFormat("en", { timeZone: "America/Blorp" });
});
