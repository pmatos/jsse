# Module loading dispatch and key design

## Context

JSSE's host resolvers currently return `PathBuf` for both real files and the
host-provided `<module source>` module. Loader callers then repeat type and
evaluation-mode dispatch, while every filesystem operation must remember that
one registry "path" is not a file. The host-loading requirements in
`HostLoadImportedModule` permit normalization and require repeated equivalent
requests to return the same Module Record, so the normalized host identity is
the stable concept the registry must represent.

This is a structural refactor. It must preserve the existing module behavior
and the current module-error caching rules.

## Approaches considered

1. Wrap only the `HashMap` key in a newtype and convert from `PathBuf` at the
   registry helpers. This is the smallest diff, but it leaves filesystem
   sentinel checks and canonicalization discipline spread across the graph.
   Deleting the wrapper would not make complexity reappear at callers, so the
   module would remain shallow.
2. Represent module identity as `enum ModuleKey { File(PathBuf), ModuleSource }`.
   This makes file access exhaustive, but adds a variant-oriented interface for
   the only host module and conflicts with the issue's explicit newtype choice.
3. Return an opaque `ModuleKey(PathBuf)` from host resolution, propagate it
   through the module graph, and make one loader dispatcher the only seam that
   converts file-backed keys to `&Path`. This gives callers the most leverage
   and concentrates host-key knowledge in one implementation. This is the
   selected approach.

## Design

`ModuleKey` is cloneable, hashable, comparable, and debuggable, but does not
implement `Deref<Target = Path>` or a total raw-path accessor. Its total
canonicalization operation preserves a host key and canonicalizes a file key,
falling back to the unresolved file path exactly as today. Its file accessor
returns `Option<&Path>`, forcing a caller to handle host identity before doing
filesystem I/O. Display remains available for diagnostics.

Both host-specifier resolution paths produce `ModuleKey`. Registry entries,
`LoadedModule` relationships, module namespace metadata, export-resolution
visited sets, DFS stacks, and async-module graph bookkeeping carry the same
type. Entry-point file paths are normalized into a Module Key before registry
insertion. General script file paths remain ordinary paths until they take
part in module resolution.

`load_module_for_type(key, import_type, mode)` is the loading interface.
`mode` distinguishes eager evaluation from deferred loading. For the host key,
the dispatcher returns the one host Module Record for untyped requests and
constructs the shared `TypeError` for text/bytes requests. For a file key, it
dispatches text, bytes, eager source-text/JSON, or deferred source-text/JSON
loading. The lower loaders accept only `&Path`, so the host key cannot reach a
file read. Existing callers retain their own module-error caching because typed
synthetic-module failures do not use that cache.

Source-phase loading stays shallow for ordinary Source Text Modules. Resolution
returns their Module Key but does not read, link, or evaluate the target. When
the resolved key names the host module, the source-phase helper goes through
the same typed loader dispatcher and reads `[[ModuleSource]]` from the returned
record. Thus the dispatcher owns the typed host rejection without making source
phase load real files.

## Failure handling

Resolver errors and filesystem/parse/link/evaluation errors retain their
current JavaScript values and timing. `load_module` continues to balance the
static-load-depth counter around every result. Eager and deferred callers keep
their existing `cache_module_error` decisions. No new fallback from a host key
to the filesystem is permitted.

## Verification

- Add focused Rust tests for total Module Key canonicalization and for the
  dispatcher rejecting typed host requests in eager and deferred modes.
- Keep the child-process regression where a real `<module source>` file cannot
  capture the host key, updating its explanation to the type-enforced model.
- Run custom tests and targeted test262 areas for dynamic import, import
  attributes, import defer, source-phase import, and module code.
- Run formatting, Clippy, release build/tests, and the full test262 suite
  against the `origin/main` baseline without rewriting it.
- Update `README.md` only if the full pass count changes.
