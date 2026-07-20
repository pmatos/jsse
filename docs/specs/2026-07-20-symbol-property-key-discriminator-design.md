# Symbol Property-Key Discriminator

## Problem

ECMAScript property keys are disjoint String and Symbol values. Every String
value is a valid property key, including strings whose text resembles a Symbol
display value. JSSE instead stores Symbol keys as ordinary `JsPropertyKey`
bytes such as `Symbol(Symbol.iterator)` or `Symbol(desc)#42` and identifies
them with `starts_with("Symbol(")`. A real String key with that prefix is
therefore misclassified, and a String that exactly matches an encoded Symbol
can alias it in property storage.

The fix must preserve exact UTF-16 String keys, keep Symbol identity distinct,
and retain the allocation-sharing and borrowed-lookup properties of the current
WTF-8-backed property map.

## Approaches Considered

1. Prefix the internal bytes of every Symbol key with a byte that canonical
   WTF-8 never emits. This preserves the current key size, hashing, equality,
   interning, and `Borrow<[u8]>` lookup seam. This is the selected approach.
2. Add a String/Symbol enum or tag field to `JsPropertyKey`. This represents the
   specification distinction directly, but a hash that includes the tag cannot
   support the existing borrowed `[u8]` lookup contract, while a hash that
   excludes it would violate Rust's `Borrow` requirements. It would also grow
   every stored key.
3. Store String and Symbol properties in separate maps. This is also explicit,
   but duplicates ordering and lookup plumbing across ordinary, exotic, and
   proxy paths and is disproportionate to this representation defect.

## Design

Reserve `0xFF` as the first byte of an internal Symbol property key. The
canonical UTF-8/WTF-8 encoder used for ECMAScript Strings never emits that
byte, so the two key domains cannot collide. `JsPropertyKey` remains a single
`Arc<[u8]>`; equality, hashing, interning, and map lookup continue to operate on
the complete stored bytes.

Add representation-level constructors and predicates so callers never inspect
the sigil directly:

- `JsSymbol::to_property_key` produces a tagged `JsPropertyKey` rather than a
  Rust `String`.
- a well-known-Symbol constructor creates the same tagged representation for
  engine bootstrap sites that do not yet have the runtime Symbol value;
- `JsPropertyKey::is_symbol` classifies keys;
- Symbol-to-value conversion reads only the tagged key's textual payload.

`as_str` continues to mean "this complete key is ordinary UTF-8 text" and
therefore returns `None` for Symbol keys. String conversion is valid only for
String keys. Debug/display compatibility may show the textual Symbol payload,
but that payload is never used for identity or classification.

Replace all raw `starts_with("Symbol(")` classifiers with `is_symbol`, and
replace all hard-coded symbol-key strings used for property access or
definition with tagged keys. Human-facing function names such as
`[Symbol.iterator]` remain ordinary text. The key interner seeds tagged
well-known keys separately from ordinary String seeds.

## Specification Semantics

The Object type defines each property key as either a String or a Symbol and
accepts every value of both types. `OrdinaryOwnPropertyKeys` emits array-index
Strings, other Strings, and Symbols as three distinct groups.
`EnumerableOwnProperties`, `GetOwnPropertyKeys`, and JSON serialization then
select keys by that type distinction. The internal discriminator implements
that distinction without exposing an engine encoding to ECMAScript code.

## Validation

Add representation unit tests proving that a Symbol key and an ordinary String
containing the same display payload are unequal and classify differently. Add
a `test262-extra` regression that keeps literal `Symbol(...)` String keys and
real Symbol keys on the same object and checks:

- bracket lookup, `in`, deletion, and descriptors;
- `Object.keys`, names, symbols, `Reflect.ownKeys`, and key ordering;
- `for-in`, `JSON.stringify`, `Object.assign`, and object spread;
- proxy key forwarding;
- both pure-ASCII and lone-surrogate String variants.

Run the focused Object, Reflect, Proxy, Symbol, JSON, and for-in test262 areas,
then the repository formatting, Clippy, release build/tests, and full test262
gate. Update the README only if the full pass count changes.
