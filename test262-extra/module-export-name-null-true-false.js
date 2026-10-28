// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-exports
description: >
  ModuleExportName is IdentifierName or StringLiteral, so the literals null,
  true and false are valid export and import names (an import binding must
  still be a valid BindingIdentifier).
info: |
  ModuleExportName :
    IdentifierName
    StringLiteral

  ImportSpecifier :
    ModuleExportName as ImportedBinding

  ExportFromClause :
    * as ModuleExportName
flags: [module]
---*/

import { null as n, true as t, false as f } from "./module-export-name-literal-fixture_FIXTURE.mjs";
import * as ns from "./module-export-name-literal-fixture_FIXTURE.mjs";
import * as re from "./module-export-name-literal-reexport_FIXTURE.mjs";

assert.sameValue(n, 1, "import { null as n }");
assert.sameValue(t, 2, "import { true as t }");
assert.sameValue(f, 3, "import { false as f }");

assert.sameValue(ns.null, 1, "namespace null");
assert.sameValue(ns.true, 2, "namespace true");
assert.sameValue(ns.false, 3, "namespace false");
assert.sameValue(ns["string name"], 1, "namespace string name");

assert.sameValue(re.null, ns, "export * as null");
assert.sameValue(re.true, ns, "export * as true");
assert.sameValue(re.false, ns, "export * as false");
assert.sameValue(re.n2, 1, "export { null as n2 } from");
assert.sameValue(re.t2, 2, "export { true as t2 } from");
assert.sameValue(re.f2, 3, "export { false as f2 } from");

assert.sameValue(
  Object.keys(re).join(),
  "f2,false,n2,null,t2,true",
  "export names enumerate in code unit order"
);
