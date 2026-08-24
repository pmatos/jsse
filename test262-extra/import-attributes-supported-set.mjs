/*---
description: Unsupported import attribute keys and type values reject
esid: sec-ImportCall-runtime-semantics-Evaluation
info: |
  HostGetSupportedImportAttributes returns the host's stable supported key
  list. AllImportAttributesSupported rejects any key outside that list before
  loading. HostLoadImportedModule also rejects unsupported values for the
  supported `type` key.
flags: [module]
features: [dynamic-import, import-attributes, top-level-await]
---*/

async function rejection(options) {
  try {
    await import('./import-attributes-javascript-dep.mjs', options);
  } catch (error) {
    return error;
  }
  return null;
}

const unknownTypeError = await rejection({ with: { type: 'bogus' } });
if (!(unknownTypeError instanceof TypeError)) {
  throw new Error('an unsupported type attribute value should reject with TypeError');
}

const unsupportedKeyError = await rejection({ with: { unsupportedKey: 'value' } });
if (!(unsupportedKeyError instanceof TypeError)) {
  throw new Error('an unsupported import attribute key should reject with TypeError');
}
