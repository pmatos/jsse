// Source-phase imports (https://github.com/tc39/proposal-source-phase-imports):
// the `<module source>` host specifier resolves to a module whose
// [[ModuleSource]] is an %AbstractModuleSource% instance, and both the static
// (`import source X from`) and dynamic (`import.source()`) forms expose it.
//
// Not covered by test262: the dynamic `import.source()` *success* path
// (test262's import.source cases are syntax-only or resolution failures) and
// the SyntaxError result of `import.source()` on an ordinary Source Text
// Module (GetModuleSource, §16.2.1.7.2).
//
// Spec:
//   - InitializeEnvironment step 7.d.v / 7.e (import source X binding)
//   - GetModuleSource: a Source Text Module has an empty [[ModuleSource]]
//   - %AbstractModuleSource% (§28.1.1)
// flags: [module]

// --- static `import source X from '<module source>'` ---
import source staticSource from '<module source>';

if (typeof staticSource !== 'object' || staticSource === null) {
  throw new Error('import source binding should be a Module Source object');
}
if (!(staticSource instanceof $262.AbstractModuleSource)) {
  throw new Error('import source binding should be a %AbstractModuleSource% instance');
}

// --- dynamic `import.source('<module source>')` resolves to a Module Source ---
const dynamicSource = await import.source('<module source>');

if (!(dynamicSource instanceof $262.AbstractModuleSource)) {
  throw new Error('import.source() should resolve to a %AbstractModuleSource% instance');
}
// The host resolves `<module source>` to the same Module Record every time,
// so both forms observe the same [[ModuleSource]] object.
if (dynamicSource !== staticSource) {
  throw new Error('import.source() and import source should yield the same [[ModuleSource]]');
}

// --- import.source() of an ordinary module rejects with SyntaxError ---
// A Source Text Module has an empty [[ModuleSource]]; GetModuleSource throws.
let rejection = null;
try {
  await import.source('./source-phase-import-module-source-plain-dep.mjs');
} catch (e) {
  rejection = e;
}
if (!(rejection instanceof SyntaxError)) {
  throw new Error('import.source() of a Source Text Module should reject with a SyntaxError, got: ' + rejection);
}
