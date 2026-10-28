// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-imports
description: >
  The local ImportedBinding of an import specifier cannot be the literal false.
info: |
  ImportSpecifier :
    ModuleExportName as ImportedBinding
negative:
  phase: parse
  type: SyntaxError
flags: [module]
---*/

$DONOTEVALUATE();

import { false as false } from "./module-export-name-literal-fixture_FIXTURE.mjs";
