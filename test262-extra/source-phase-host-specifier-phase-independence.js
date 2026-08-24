/*---
description: >
  Host module loading must not let the import phase decide whether a specifier
  resolves, nor how an unsatisfiable request fails. The `<module source>` host
  specifier must resolve to one Module Record for the source phase
  (`import source X from`, `import.source()`) and the evaluation phase
  (`import * as ns from`, `import defer * as ns from`, bare `import`,
  `export * from`, `import()`) alike, and a request carrying an import type the
  host cannot honour must be rejected identically in both phases.
esid: sec-HostLoadImportedModule
info: |
  test262/INTERPRETING.md:
    Implementers should resolve the specifier `<module source>` to a module that
    provides a valid Module Source, such as a WebAssembly module. Tests use
    `<module source>` specifier are guarded with a feature flag
    `source-phase-imports-module-source`.

  So the host specifier names a *module*, reachable in either phase — not a
  source-phase-only stand-in.

  16.2.1.8 HostLoadImportedModule ( referrer, moduleRequest, hostDefined, payload )
    An implementation of HostLoadImportedModule must conform to the following
    requirements:
      [...]
      * If this operation is called multiple times with two (referrer,
        moduleRequest) pairs such that:
          - the first referrer is the same as the second referrer;
          - ModuleRequestsEqual(the first moduleRequest, the second
            moduleRequest) is true;
        and it performs FinishLoadingImportedModule(referrer, moduleRequest,
        payload, result) where result is a normal completion, then it must
        perform FinishLoadingImportedModule(referrer, moduleRequest, payload,
        result) with the same result each time.

  16.2.1.3 ModuleRequestsEqual ( left, right )
    1. If left.[[Specifier]] is not right.[[Specifier]], return false.
    [... attribute comparison ...]
    8. Return true.

  Module identity is therefore keyed on specifier and attributes. The request's
  phase is not part of that key: ECMA-262 has no [[Phase]] field at all, and the
  source-phase-imports proposal, which introduces it
  (https://github.com/tc39/proposal-source-phase-imports), does not add it to
  ModuleRequestsEqual. Attributes *are* part of the key, so a typed request is a
  distinct request the host may reject — but it must reject it the same way
  whichever phase issued it.

  Not covered by test262: every upstream use reaches `<module source>` through
  the source phase only. Of the 105 files under
  dynamic-import/syntax/{valid,invalid}, 63 are `phase: parse` negatives and the
  other 42 evaluate a bare `import.source(...)`; three module-code tests reach it
  at runtime (ambiguous-export-bindings/namespace-unambiguous-if-import-source-and-export.js
  and source-phase-import/reexport-source-binding-{named-import,namespace-get}.js,
  the latter two via reexport-source-binding_FIXTURE.js). None issues an
  evaluation-phase `import('<module source>')`, so nothing upstream catches a
  host resolver that recognises the specifier in the source phase alone.
  See jsse#222.
flags: [module]
features: [source-phase-imports, source-phase-imports-module-source, dynamic-import, import-defer, import-attributes, top-level-await]
---*/

import source staticSource from '<module source>';
import * as staticNs from '<module source>';
import defer * as deferredNs from '<module source>';
import '<module source>';
export * from '<module source>';

// --- the source phase yields the [[ModuleSource]] ---
assert.sameValue(
  typeof staticSource,
  'object',
  'import source binding is a Module Source object'
);
assert.notSameValue(staticSource, null, 'import source binding is not null');

// --- the evaluation phase resolves the same specifier ---
assert.sameValue(typeof staticNs, 'object', 'import * as resolves the host specifier');
assert.sameValue(
  Object.getPrototypeOf(staticNs),
  null,
  'a module namespace has a null [[Prototype]]'
);
assert.sameValue(
  staticNs[Symbol.toStringTag],
  'Module',
  'a module namespace has Symbol.toStringTag "Module"'
);
assert.sameValue(Object.isExtensible(staticNs), false, 'a module namespace is not extensible');
assert.sameValue(Object.keys(staticNs).length, 0, 'the host module exposes no bindings');

assert.sameValue(typeof deferredNs, 'object', 'import defer * as resolves the host specifier');
assert.sameValue(
  Object.keys(deferredNs).length,
  0,
  'the deferred namespace exposes no bindings'
);

// --- both phases name one Module Record ---
// GetModuleNamespace caches [[Namespace]] on the record and GetModuleSource reads
// [[ModuleSource]] off it, so identity across phases is the observable proof that
// one record backs both.
const dynamicNs = await import('<module source>');
const dynamicSource = await import.source('<module source>');

assert.sameValue(dynamicNs, staticNs, 'import() and import * as observe the same [[Namespace]]');
assert.sameValue(
  dynamicSource,
  staticSource,
  'import.source() and import source observe the same [[ModuleSource]]'
);

// A repeat request must not mint a second record.
assert.sameValue(
  await import('<module source>'),
  staticNs,
  'a repeated import() resolves to the same record'
);
assert.sameValue(
  await import.source('<module source>'),
  staticSource,
  'a repeated import.source() resolves to the same record'
);

// --- an unsatisfiable typed request fails identically in both phases ---
// The host has no JSON, text, or bytes for a Module Source module. Attributes
// are part of the module request, so this is a distinct request the host may
// reject; what it must not do is reject in one phase and resolve in the other.
const dynamicForms = {
  'import()': (s, o) => import(s, o),
  'import.source()': (s, o) => import.source(s, o),
  'import.defer()': (s, o) => import.defer(s, o),
};

async function rejection(fn, specifier, options) {
  try {
    await fn(specifier, options);
  } catch (e) {
    return e;
  }
  return null;
}

// Cases are flattened into one list because `await` inside a loop nested in
// another loop loses the outer iteration's bindings on jsse (see jsse#476);
// a single loop level is unaffected.
const typedCases = [];
for (const type of ['json', 'text', 'bytes']) {
  for (const name of Object.keys(dynamicForms)) {
    typedCases.push([name, type]);
  }
}

const messagesByType = { json: [], text: [], bytes: [] };
for (const [name, type] of typedCases) {
  const err = await rejection(dynamicForms[name], '<module source>', { with: { type } });
  assert(
    err instanceof TypeError,
    name + ' rejects a type: ' + type + ' request with a TypeError, got: ' + err
  );
  messagesByType[type].push(err.message);
}

for (const type of Object.keys(messagesByType)) {
  assert.sameValue(
    new Set(messagesByType[type]).size,
    1,
    'every form rejects a type: ' + type + ' request with the same message, got: ' +
      JSON.stringify(messagesByType[type])
  );
}

// --- attributes come from enumerable own properties of `with` only ---
// EvaluateImportCall enumerates `with`, so an inherited or non-enumerable `type`
// is not an import attribute and must not turn into a rejection. Reading `type`
// straight off the object instead of enumerating gets this wrong.
const inheritedType = Object.create({ type: 'text' });
const nonEnumerableType = {};
Object.defineProperty(nonEnumerableType, 'type', { value: 'text', enumerable: false });

const attributeCases = [];
for (const [label, withValue] of [
  ['an inherited', inheritedType],
  ['a non-enumerable own', nonEnumerableType],
]) {
  for (const name of Object.keys(dynamicForms)) {
    attributeCases.push([label, withValue, name]);
  }
}

for (const [label, withValue, name] of attributeCases) {
  const err = await rejection(dynamicForms[name], '<module source>', { with: withValue });
  assert.sameValue(
    err,
    null,
    name + ' must ignore ' + label + ' `type` on `with`, but rejected with: ' + err
  );
}

// --- reaching the record through the evaluation phase adds no bindings ---
const ownKeys = Reflect.ownKeys(staticNs);
assert.sameValue(ownKeys.length, 1, 'the namespace has exactly one own key');
assert.sameValue(ownKeys[0], Symbol.toStringTag, 'the only own key is Symbol.toStringTag');
