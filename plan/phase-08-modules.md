# Phase 8: Modules & Scripts

**Spec Reference:** §16 (Scripts and Modules)

## Goal
Implement script evaluation, ES module loading/linking/evaluation, and `import()`/`import.meta`.

## Tasks

### 8.1 Scripts (§16.1)
- [ ] ParseScript
- [ ] ScriptEvaluation
- [ ] GlobalDeclarationInstantiation
- [ ] Script Records

### 8.2 Module Semantics (§16.2)
- [ ] Module Records (abstract)
- [ ] Cyclic Module Records
  - [ ] Link / Evaluate / ExecuteModule
  - [ ] InnerModuleLinking / InnerModuleEvaluation
  - [ ] Module status lifecycle (new → linking → linked → evaluating → evaluated)
  - [ ] Cycle detection and handling
- [ ] Source Text Module Records
  - [ ] ParseModule
  - [ ] GetExportedNames
  - [ ] ResolveExport (star export resolution, ambiguity detection)
  - [ ] InitializeEnvironment (import binding creation)
  - [ ] ExecuteModule

### 8.3 Import Declarations (§16.2.2)
- [ ] Named imports `import { a, b } from '...'`
- [ ] Default imports `import x from '...'`
- [ ] Namespace imports `import * as ns from '...'`
- [ ] Side-effect imports `import '...'`
- [ ] Import attributes `import x from '...' with { type: 'json' }`

### 8.4 Export Declarations (§16.2.3)
- [ ] Named exports `export { a, b }`
- [ ] Default export `export default ...`
- [ ] Variable/function/class exports
- [ ] Re-exports `export { a } from '...'`
- [ ] Star re-export `export * from '...'`
- [ ] Namespace re-export `export * as ns from '...'`

### 8.5 Dynamic Import (§13.3.10)
- [ ] `import()` expression → Promise
- [ ] ContinueDynamicImport
- [ ] FinishDynamicImport

### 8.6 `import.meta` (§13.3.12)
- [ ] HostGetImportMetaProperties
- [ ] `import.meta.url`

### 8.7 Host Hooks
- [ ] HostResolveImportedModule
- [ ] HostLoadImportedModule
- [ ] HostImportModuleDynamically
- [ ] HostGetSupportedImportAttributes
- [ ] Module specifier resolution (file path, URL, bare specifier)

### 8.8 Top-Level Await
- [ ] Async module evaluation
- [ ] Async module dependency graph handling

### 8.9 Module Namespace Objects (§10.4.6)
- [ ] Module namespace exotic object
- [ ] `[[Get]]`, `[[GetOwnProperty]]`, `[[OwnPropertyKeys]]`, `[[HasProperty]]`
- [ ] `[Symbol.toStringTag]`: `"Module"`

## test262 Tests
- `test262/test/language/module-code/` — 737 tests
- `test262/test/language/import/` — 162 tests
- `test262/test/language/export/` — 3 tests
- `test262/test/language/expressions/dynamic-import/` — 995 tests
- `test262/test/language/expressions/import.meta/` — 23 tests
- `test262/test/built-ins/AbstractModuleSource/` — 8 tests
