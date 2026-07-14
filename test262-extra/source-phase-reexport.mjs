// Source-phase imports: re-exporting a source-phase binding
// (`import source x from '...'; export { x };`) reclassifies the export to an
// indirect ExportEntry whose [[ImportName]] is ~source~. Resolving it yields a
// ResolvedBinding { [[Module]]: source-phase target, [[BindingName]]: ~source~ }
// which binds to the target's [[ModuleSource]].
//
// This is a local regression guard for the fix tracked by pmatos/jsse#181,
// mirroring the three test262 cases:
//   language/module-code/source-phase-import/reexport-source-binding-named-import.js
//   language/module-code/source-phase-import/reexport-source-binding-namespace-get.js
//   language/module-code/ambiguous-export-bindings/namespace-unambiguous-if-import-source-and-export.js
//
// Spec:
//   - ResolveExport ~source~ (sec-resolveexport)
//   - Module Namespace [[Get]] ~source~ (sec-module-namespace-...-get)
// flags: [module]

// A named import of a re-exported source-phase binding binds to the
// underlying [[ModuleSource]].
import { x } from './source-phase-reexport-dep.mjs';
// A namespace import observes the same [[ModuleSource]] via [[Get]].
import * as ns from './source-phase-reexport-dep.mjs';
// Re-exporting the same `<module source>` binding from two modules through
// `export *` is unambiguous: both resolve to the same [[Module]] + ~source~.
import { mod } from './source-phase-reexport-star-dep.mjs';

if (!(x instanceof $262.AbstractModuleSource)) {
  throw new Error('named re-exported source binding should be a %AbstractModuleSource% instance');
}
if (!(ns.x instanceof $262.AbstractModuleSource)) {
  throw new Error('namespace [[Get]] of a re-exported source binding should be a %AbstractModuleSource% instance');
}
if (ns.x !== x) {
  throw new Error('the named and namespace views should observe the same [[ModuleSource]]');
}
if (!(mod instanceof $262.AbstractModuleSource)) {
  throw new Error('unambiguous double star re-export should resolve to a %AbstractModuleSource% instance');
}
if (mod !== x) {
  throw new Error('all re-exports of the same `<module source>` should observe the same [[ModuleSource]]');
}
