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
attribute keys and values before host loading. Static requests pass through
that seam individually during their source-order host-resolution pass, before
the separate linking/loading pass, matching `InnerModuleLoading` rather than
pre-scanning only attributes for the whole module:

- Dynamic imports reject unsupported keys and values with `TypeError`.
- Static module requests reject unsupported keys with the resolution-phase
  `SyntaxError` required by `AllImportAttributesSupported`; unsupported values
  reach the host selector and throw `TypeError`.
- Key support is checked over the complete attribute list before any supported
  `type` value is interpreted, so the error kind is independent of attribute
  source order.

Typed resources are loaded through one loader. JSON uses the existing JSON
parser and the existing default-export synthetic-module representation, and is
memoized like text and byte modules — by realm, canonical resource path, and
normalized module type. Realm is part of the key because each realm has its own
module map and its own intrinsics; a JSON module loaded inside a ShadowRealm
must parse into that realm's `Object.prototype`. Untyped `.json` requests call
the same loader, retaining current host behavior and identity.

A re-export carrying an attribute cannot round-trip through the `*reexport:` /
`*ns:` binding formats, which record only a specifier and lose the request's
type. Such re-exports are materialized as internal module-env bindings
(`*synthetic-reexport:x*`, `*synthetic-ns:x*`) before imports are processed, so
they are resolvable through any namespace built during loading and cannot
collide with a same-named local declaration.

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
- An unsupported key takes precedence over an unsupported `type` value in the
  same request, while an earlier request's resolution error takes precedence
  over a later request's attribute error.
- `<module source>` rejects a JSON request.
- A typed re-export resolves through a namespace built during loading, and does
  not alias a same-named local declaration.
- A JSON module loaded in a ShadowRealm uses that realm's intrinsics.

The upstream import-attributes directories validate existing JSON and text
module behavior, syntax, idempotency, and static resolution failures.
