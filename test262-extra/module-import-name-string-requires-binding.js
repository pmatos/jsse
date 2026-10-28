// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-imports
description: >
  `import { "a" }` has no `as`, so there is no ImportedBinding and the
  specifier is a SyntaxError.
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

import { "a" } from "./module-export-name-literal-fixture_FIXTURE.mjs";
