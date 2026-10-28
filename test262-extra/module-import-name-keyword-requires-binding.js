// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-imports
description: >
  `import { default }` has no `as`, so the reserved word would be the
  ImportedBinding, which is a SyntaxError.
info: |
  ImportSpecifier :
    ImportedBinding
    ModuleExportName as ImportedBinding
negative:
  phase: parse
  type: SyntaxError
flags: [module]
---*/

$DONOTEVALUATE();

import { default } from "./module-export-name-literal-fixture_FIXTURE.mjs";
