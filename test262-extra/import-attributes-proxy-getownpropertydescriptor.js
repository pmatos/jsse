/*---
description: >
  Dynamic import attributes consult a Proxy's getOwnPropertyDescriptor trap
  when the Proxy has no ownKeys trap
features: [source-phase-imports, source-phase-imports-module-source, dynamic-import, import-defer, import-attributes, Proxy, top-level-await]
flags: [module]
---*/

// EvaluateImportCall calls EnumerableOwnProperties on the `with` object.
// EnumerableOwnProperties first calls O.[[OwnPropertyKeys]](), then calls
// O.[[GetOwnProperty]](key) for each returned string key. A Proxy without an
// ownKeys trap delegates only the first operation to its target; the second
// operation must still consult the Proxy's getOwnPropertyDescriptor trap.
//
// Spec:
//   - EvaluateImportCall, attributes enumeration
//   - EnumerableOwnProperties, steps 1 and 3.a.i
//   - Proxy [[OwnPropertyKeys]], step 6
//   - Proxy [[GetOwnProperty]], steps 5-7

const target = { type: 'text' };
let descriptorCalls = 0;
const withValue = new Proxy(target, {
  getOwnPropertyDescriptor(target, key) {
    descriptorCalls++;
    const descriptor = Reflect.getOwnPropertyDescriptor(target, key);
    if (key === 'type' && descriptor) {
      descriptor.enumerable = false;
    }
    return descriptor;
  },
});

const dynamicForms = {
  'import()': (specifier, options) => import(specifier, options),
  'import.source()': (specifier, options) => import.source(specifier, options),
  'import.defer()': (specifier, options) => import.defer(specifier, options),
};

for (const name of Object.keys(dynamicForms)) {
  let error = null;
  try {
    await dynamicForms[name]('<module source>', { with: withValue });
  } catch (e) {
    error = e;
  }
  assert.sameValue(
    error,
    null,
    name + ' must ignore a key hidden by the Proxy descriptor trap, but rejected with: ' + error
  );
}

assert.sameValue(
  descriptorCalls,
  3,
  'each import form must consult the Proxy descriptor trap exactly once'
);
