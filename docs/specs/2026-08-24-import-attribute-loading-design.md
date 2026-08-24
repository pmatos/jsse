# Import Attribute Loading Design

## Scope

Fix module requests carrying `type: "json"` so their resource text is parsed
with `ParseJSONModule` semantics regardless of the resource extension. Define
the host's supported import-attribute keys as the stable list `type`, reject
unsupported keys, and reject unsupported `type` values rather than treating
either case as an untyped source-module request.

Existing untyped `.json` loading remains supported, as ECMA-262 explicitly
permits hosts to serve JSON modules without a `type: "json"` attribute.

## Design

`ImportModuleType` is the host's normalized module-type selector and gains a
`Json` variant alongside `Text` and `Bytes`. A single validation seam checks
attribute keys and values before host loading:

- Dynamic imports reject unsupported keys and values with `TypeError`.
- Static module requests reject unsupported keys with the resolution-phase
  `SyntaxError` required by `AllImportAttributesSupported`; unsupported values
  reach the host selector and throw `TypeError`.

Typed resources are loaded through one dispatcher. JSON uses the existing JSON
parser and the existing default-export synthetic-module representation, but is
cached by canonical resource path and normalized module type like text and byte
modules. Untyped `.json` requests call the same JSON loader, retaining current
host behavior and identity.

The `<module source>` host resource has no JSON representation, so the shared
typed-resource error reports a `TypeError` for `json`, `text`, and `bytes` in
every dynamic import phase.

## Alternatives

1. Keep extension dispatch and special-case `type: "json"` at each call site.
   This is smaller initially but leaves static, dynamic, deferred, and source
   phases free to diverge again.
2. Carry arbitrary raw attribute maps through every loader and registry key.
   This is maximally general but expands the module identity model beyond the
   issue's one supported key and three supported values.
3. Normalize validated attributes to a typed selector and share the loader.
   This is the chosen approach because it closes every current loading path
   without redesigning unrelated module-record storage.

## Error Handling

JSON parse failures propagate the existing `SyntaxError` completion. Resource
I/O failures preserve the existing host error. An unknown attribute key never
reaches the loader. An unknown `type` value never falls back to source text.

## Tests

Custom module tests cover:

- JavaScript source requested as JSON rejects with the JSON parse error.
- Valid JSON with a non-`.json` extension loads and exports its parsed value.
- Unknown keys and type values reject.
- `<module source>` rejects a JSON request.

The upstream import-attributes directories validate existing JSON and text
module behavior, syntax, idempotency, and static resolution failures.
