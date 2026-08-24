/*---
description: Static module requests are validated and loaded in source order
esid: sec-InnerModuleLoading
info: |
  InnerModuleLoading handles each ModuleRequest in source order. A resolution
  failure for an earlier request must therefore win over unsupported import
  attributes on a later request.
flags: [module]
features: [dynamic-import, import-attributes, top-level-await]
---*/

let error;
try {
  await import('./import-attributes-request-order-dep_FIXTURE.mjs');
} catch (caught) {
  error = caught;
}

assert.sameValue(
  typeof error,
  'string',
  'the earlier host resolution failure must win over the later SyntaxError'
);
assert.sameValue(
  error.includes("Cannot find module './import-attributes-request-order-missing.mjs'"),
  true,
  'the rejection must identify the first request'
);
