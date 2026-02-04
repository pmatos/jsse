# Phase 8: Modules & Scripts

**Spec Reference:** §16 (Scripts and Modules)
**Status:** ✅ ~90% Complete

## Goal
Implement script evaluation, ES module loading/linking/evaluation, and `import()`/`import.meta`.

## Tasks

### 8.1 Scripts (§16.1)
- [x] ParseScript
- [x] ScriptEvaluation
- [x] GlobalDeclarationInstantiation
- [x] Script Records

### 8.2 Module Semantics (§16.2)
- [x] Module Records (abstract)
- [x] Cyclic Module Records
  - [x] Link / Evaluate / ExecuteModule
  - [x] InnerModuleLinking / InnerModuleEvaluation
  - [x] Module status lifecycle (new → linking → linked → evaluating → evaluated)
  - [x] Cycle detection and handling
  - [x] Circular re-export detection
- [x] Source Text Module Records
  - [x] ParseModule
  - [x] GetExportedNames
  - [x] ResolveExport (star export resolution, ambiguity detection)
  - [x] InitializeEnvironment (import binding creation)
  - [x] ExecuteModule

### 8.3 Import Declarations (§16.2.2)
- [x] Named imports `import { a, b } from '...'`
- [x] Default imports `import x from '...'`
- [x] Namespace imports `import * as ns from '...'`
- [x] Side-effect imports `import '...'`
- [ ] Import attributes `import x from '...' with { type: 'json' }`

### 8.4 Export Declarations (§16.2.3)
- [x] Named exports `export { a, b }`
- [x] Default export `export default ...`
- [x] Variable/function/class exports
- [x] Re-exports `export { a } from '...'`
- [x] Star re-export `export * from '...'`
- [x] Namespace re-export `export * as ns from '...'`
- [x] Duplicate export detection (parse-time)
- [x] Re-export live binding resolution

### 8.5 Dynamic Import (§13.3.10)
- [x] `import()` expression → Promise
- [x] ContinueDynamicImport
- [x] FinishDynamicImport

### 8.6 `import.meta` (§13.3.12)
- [x] HostGetImportMetaProperties
- [x] `import.meta.url`

### 8.7 Host Hooks
- [x] HostResolveImportedModule
- [x] HostLoadImportedModule
- [x] HostImportModuleDynamically
- [ ] HostGetSupportedImportAttributes
- [x] Module specifier resolution (file path, relative paths)
- [ ] Bare specifier resolution (node_modules style)

### 8.8 Top-Level Await
- [x] Async module evaluation
- [x] Async module dependency graph handling
- [x] `await` allowed at module top-level

### 8.9 Module Namespace Objects (§10.4.6)
- [x] Module namespace exotic object
- [x] `[[Get]]` with live binding lookup
- [x] `[[GetOwnProperty]]`
- [x] `[[OwnPropertyKeys]]` (sorted alphabetically)
- [x] `[[HasProperty]]`
- [x] `[Symbol.toStringTag]`: `"Module"`
- [x] Non-extensible namespace
- [x] Null prototype
- [x] Re-export binding resolution

## test262 Tests
- `test262/test/language/module-code/` — 518/737 (70%)
- `test262/test/language/import/` — 120/162 (74%)
- `test262/test/language/export/` — 3 tests
- `test262/test/language/expressions/dynamic-import/` — ~60%
- `test262/test/language/expressions/import.meta/` — ~80%

## Implementation Notes

### Module Loading Flow
1. `run_module()` or `load_module()` is called with a module path
2. Module is parsed with `parse_program_as_module()` (strict mode, checks for duplicate exports)
3. Module is registered in `module_registry` early for circular import handling
4. Export bindings collected before imports processed
5. Imports resolved and bindings created
6. Module body executed
7. Export values collected after execution

### Re-export Resolution
Re-exports use a special binding format `*reexport:source:name` that is resolved dynamically during namespace property access:
1. Parse the binding format to extract source specifier and export name
2. Resolve source module from current module's path
3. Look up export through source module's export_bindings for live binding
4. Fall back to environment lookup, then exports map

### Circular Dependencies
- Modules registered early to handle circular imports
- Circular re-exports detected with visited set tracking
- SyntaxError thrown for circular re-export chains
