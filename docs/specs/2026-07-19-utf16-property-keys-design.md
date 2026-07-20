# UTF-16-Preserving Property Keys

## Problem

ECMAScript strings are sequences of arbitrary 16-bit code units, and every
String value is a valid property key. JSSE already represents language String
values as `JsString`, but property-key seams and object storage use Rust
`String`. Converting a lone surrogate through `String::from_utf16_lossy`
replaces it with U+FFFD, so distinct keys such as `"\uFFFD"`, `"\uD834"`, and
`"\uDF06"` collapse.

The implementation must preserve exact code units through parsing,
`ToPropertyKey`, property storage and lookup, ordered enumeration, and proxy
trap forwarding. Existing symbol behavior is outside this issue's scope.

## Design

Introduce a dedicated internal `JsPropertyKey` whose backing storage is
canonical WTF-8 bytes:

- well-formed UTF-16 is encoded as ordinary UTF-8;
- paired surrogates use the canonical four-byte UTF-8 encoding;
- lone surrogates use their three-byte WTF-8 encoding.

This representation is injective over ECMAScript String values and round-trips
to `JsString` without replacement. Ordinary property names retain their UTF-8
bytes, allowing the property map to continue serving `&str` lookups without an
allocation via `Borrow<[u8]>`. Key interning, `PropertyMap`, and
`property_order` will store the dedicated type so exactness is enforced at the
storage seam.

The AST's string-literal property-key variant will retain the lexer's
`Vec<u16>` instead of converting it lossily during parsing. Evaluation of both
literal and computed property names will produce `JsPropertyKey`. Property MOP
operations will accept exact keys while preserving ergonomic `&str` calls for
engine-internal ASCII names.

Enumeration and proxy paths will convert stored keys back to `JsString`, never
through lossy Rust text. Array-index and well-known internal-name checks apply
only when the key bytes are valid UTF-8; a key containing a lone surrogate is
therefore correctly treated as an ordinary string key.

## Alternatives Considered

1. Store `JsString` directly. This is semantically direct, but Rust's hash-map
   borrowed lookup cannot compare a `&str` with a `Vec<u16>` key, making common
   built-in lookups allocate or requiring parallel maps.
2. Escape lone surrogates into a Rust `String`. A reversible escape scheme can
   be made collision-free, but it keeps an encoded `String` at the unsafe seam
   and relies on every output path remembering to decode it.

WTF-8 provides exactness, type safety, compact storage, and compatibility with
the common well-formed key path.

## Validation

Add a `test262-extra` regression covering distinct U+FFFD/high-surrogate/
low-surrogate keys across object literals and computed definitions, bracket
lookup, assignment, deletion and re-addition, `in`, own-property checks,
property descriptors, `Object.keys`, `Object.getOwnPropertyNames`,
`Reflect.ownKeys`, and proxy key traps.

Run the focused object-expression and Object/Reflect key suites, then the
repository's complete formatting, linting, release build, release tests, and
full test262 gate. Update the README only if the full pass count changes.
