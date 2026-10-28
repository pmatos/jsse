// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-imports
description: >
  `import { null }` has no `as`, so the name doubles as the ImportedBinding,
  and null is not a valid BindingIdentifier.
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

import { null } from "./module-export-name-literal-fixture_FIXTURE.mjs";
