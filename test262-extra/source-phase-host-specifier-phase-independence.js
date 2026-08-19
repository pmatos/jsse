/*---
description: >
  Host module loading must not let the import phase decide whether a specifier
  resolves. ModuleRequestsEqual compares only [[Specifier]] and [[Attributes]] —
  never the request's phase — so HostLoadImportedModule must hand back the same
  Module Record for the source phase (`import source X from`,
  `import.source()`) and for the evaluation phase (`import * as ns from`,
  `import defer * as ns from`, `import()`). INTERPRETING.md requires the host
  specifier `<module source>` to resolve to a *module* providing a Module Source,
  so it must resolve in every phase, with all phases observing one record.
esid: sec-HostLoadImportedModule
info: |
  16.2.1.8 HostLoadImportedModule ( referrer, moduleRequest, hostDefined, payload )

  An implementation of HostLoadImportedModule must conform to the following
  requirements:
    [...]
    * If this operation is called multiple times with two (referrer,
      moduleRequest) pairs such that:
        - the first referrer is the same as the second referrer;
        - ModuleRequestsEqual(the first moduleRequest, the second moduleRequest)
          is true;
      and it performs FinishLoadingImportedModule(referrer, moduleRequest,
      payload, result) where result is a normal completion, then it must perform
      FinishLoadingImportedModule(referrer, moduleRequest, payload, result) with
      the same result each time.

  16.2.1.3 ModuleRequestsEqual ( left, right )
    1. If left.[[Specifier]] is not right.[[Specifier]], return false.
    [... attribute comparison ...]
    8. Return true.

  The phase of the request is not an input to ModuleRequestsEqual, so a
  host-provided specifier that resolves under `import.source()` must equally
  resolve under `import()`.

  test262/INTERPRETING.md:
    Implementers should resolve the specifier `<module source>` to a module that
    provides a valid Module Source, such as a WebAssembly module. Tests use
    `<module source>` specifier are guarded with a feature flag
    `source-phase-imports-module-source`.

  Not covered by test262: every upstream test reaches `<module source>` through
  the source phase only — 105 parse-phase cases under
  dynamic-import/syntax/{valid,invalid} that never evaluate, plus
  module-code/ambiguous-export-bindings/namespace-unambiguous-if-import-source-and-export.js,
  whose fixtures `import source ... from '<module source>'` and re-export it.
  None performs an evaluation-phase `import('<module source>')`, so nothing
  upstream catches a host resolver that recognises the specifier in the source
  phase alone. See jsse#222.
flags: [module]
features: [source-phase-imports, source-phase-imports-module-source, dynamic-import, import-defer]
---*/

import source staticSource from '<module source>';
import * as staticNs from '<module source>';
import defer * as deferredNs from '<module source>';

// --- the source phase still yields the [[ModuleSource]] ---
assert.sameValue(
  typeof staticSource,
  'object',
  'import source binding is a Module Source object'
);
assert.notSameValue(staticSource, null, 'import source binding is not null');

// --- the evaluation phase resolves the same specifier ---
// Before this was fixed, `import * as ns from '<module source>'` and
// `import('<module source>')` failed host resolution ("Cannot resolve bare
// module specifier") while the source phase succeeded.
assert.sameValue(
  typeof staticNs,
  'object',
  'import * as resolves the host specifier'
);
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
assert.sameValue(
  Object.isExtensible(staticNs),
  false,
  'a module namespace is not extensible'
);
assert.sameValue(
  Object.keys(staticNs).length,
  0,
  'the host module exposes no bindings'
);

assert.sameValue(
  typeof deferredNs,
  'object',
  'import defer * as resolves the host specifier'
);
assert.sameValue(
  Object.keys(deferredNs).length,
  0,
  'the deferred namespace exposes no bindings'
);

// --- both phases name one Module Record ---
// GetModuleNamespace caches [[Namespace]] on the record and GetModuleSource
// reads [[ModuleSource]] off it, so identity across the two phases is the
// observable proof that one record backs both.
const dynamicNs = await import('<module source>');
const dynamicSource = await import.source('<module source>');

assert.sameValue(
  dynamicNs,
  staticNs,
  'import() and import * as observe the same [[Namespace]]'
);
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

// --- an import-type attribute the host cannot honour must not reach the disk ---
// ModuleRequestsEqual compares [[Attributes]] too, so `('<module source>', text)`
// is a *different* module request; the host has no text/bytes representation for a
// Module Source module and must throw rather than fall through to a file read.
for (const type of ['text', 'bytes']) {
  let err = null;
  try {
    await import('<module source>', { with: { type } });
  } catch (e) {
    err = e;
  }
  assert.notSameValue(err, null, 'import(<module source>, type: ' + type + ') rejects');
  assert.sameValue(
    String(err).indexOf('No such file') >= 0,
    false,
    'the rejection for type: ' + type + ' must not leak a filesystem error'
  );
}

// --- reaching the record through the evaluation phase adds no bindings ---
// The host module has no exports, so its namespace carries Symbol.toStringTag
// and nothing else, whichever phase materialised it.
const ownKeys = Reflect.ownKeys(staticNs);
assert.sameValue(ownKeys.length, 1, 'the namespace has exactly one own key');
assert.sameValue(
  ownKeys[0],
  Symbol.toStringTag,
  'the only own key is Symbol.toStringTag'
);
