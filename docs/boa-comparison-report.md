# JSSE vs Boa Code Originality Report

**Date**: 2026-03-26
**Methodology**: 9 parallel investigation agents each performed deep code-level comparison of a major subsystem, reading both codebases and comparing architecture, data structures, algorithms, naming conventions, comments, and specific code blocks.

## Overall Verdict: CLEARLY INDEPENDENT

**There is no evidence of code copying from Boa to JSSE in any area examined.** The two codebases are fundamentally different implementations of the same ECMAScript specification. Every similarity found is attributable to spec-mandated algorithms, shared Rust ecosystem crates, or universal programming idioms.

---

## Summary Table

| Area | JSSE | Boa | Similarity Rating |
|------|------|-----|-------------------|
| **GC** | 469-line single file, `Vec<Option<Rc<RefCell>>>` + free list, manual root enumeration | 4,300-line multi-file crate, `Gc<T>` smart pointers, trait-based tracing with derive macros | **Clearly independent** |
| **Lexer** | 1,772-line single file, `&str` + `Chars`, flat `Token` enum with ~60 punctuator variants | 4,347-line modular design, `Tokenizer` trait per token category, string interning, `Cursor<R>` abstraction | **Clearly independent** |
| **Parser** | 7,661 lines across 5 files, classic single-struct recursive descent with `parse_X()` methods, mutable state fields | 16,415 lines across 90+ files, `TokenParser` trait with struct-per-production, macro-generated precedence levels | **Clearly independent** |
| **AST/Types** | 474-line single file, flat enums with inline data, `String` identifiers | 23,400-line multi-file crate, newtype wrappers, interned `Sym` handles, visitor pattern, spans | **Clearly independent** |
| **Interpreter Core** | Tree-walking, HashMap-keyed environments, monolithic `JsObjectData` god-struct (~50 fields), `Completion` enum | Register-based bytecode VM, index-resolved environments, shape-based polymorphic objects, `JsResult<T>` | **Clearly independent** |
| **Temporal** | 23,123 lines from scratch, hand-written ISO parsers, direct ICU4X calendar arithmetic, no `temporal_rs` | 11,713 lines, thin wrapper around `temporal_rs` crate for all core logic | **Clearly independent** |
| **Intl (ECMA-402)** | String-based `IntlData` enum, ICU4X formatters created on-the-fly, has DisplayNames/DurationFormat/RelativeTimeFormat | Typed structs with persistent ICU4X objects, `Service` trait abstraction, GC-traced formatters | **Clearly independent** |
| **Collections/Iterators** | `Vec<Option<(K,V)>>` for Map/Set, `IteratorState` enum, replay-based generators | `OrderedMap` with `IndexMap`, dedicated typed structs per iterator, VM context-save generators | **Clearly independent** |
| **Core Builtins** (Array, String, Number, Math, JSON, RegExp, Promise, Date) | Hand-written JSON parser, custom regex compiler (9,109 lines), `format!` for number formatting, closure-based registration | `serde_json` + bytecode for JSON, `regress` crate for RegExp, `ryu_js` for formatting, `BuiltInBuilder` pattern | **Clearly independent** |

---

## Key Architectural Differences

These pervasive differences make code copying structurally impossible:

1. **Execution model**: JSSE is a tree-walking interpreter; Boa compiles to bytecode and runs a register-based VM
2. **Object model**: JSSE uses a single `JsObjectData` struct with ~50 optional fields and `Rc<RefCell<>>` sharing; Boa uses typed `JsObject` handles with GC tracing, shapes (hidden classes), and `downcast::<T>()`
3. **Error handling**: JSSE uses a `Completion` enum (`Normal`/`Throw`/`Return`/`Break`/`Continue`); Boa uses Rust's `Result<T, JsError>` with the `?` operator
4. **Memory management**: JSSE uses `Rc<RefCell<>>` with a hand-rolled mark-and-sweep GC (469 lines); Boa uses a standalone `boa_gc` crate with `Trace`/`Finalize` derive macros
5. **String representation**: JSSE uses `JsString { code_units: Vec<u16> }`; Boa uses an interned `JsString` from the `boa_string` crate with Latin1/UTF-16 optimization
6. **Function registration**: JSSE uses `create_function(JsFunction::native(...))` closures; Boa uses `BuiltInBuilder` fluent API with static methods
7. **Type checking**: JSSE checks `class_name` strings (e.g., `"Map"`, `"WeakMap"`); Boa uses typed downcasting (`obj.downcast_ref::<Map>()`)
8. **Generator implementation**: JSSE uses AST-to-state-machine transformation; Boa saves/restores VM execution context
9. **Environment/scope**: JSSE uses `HashMap<String, Binding>` with runtime name resolution; Boa uses compile-time binding index resolution

## What Similarities Exist (and Why)

The only similarities found across all 9 investigations are:

- **Spec-mandated algorithms**: Both implement `ToBoolean`, `ToNumber`, `SameValue`, `MakeTime`, `MakeDay`, etc. identically because the ECMAScript spec prescribes exact step-by-step algorithms. Any conformant implementation must follow these steps.
- **Shared Rust crates**: Both use `num_bigint` for BigInt, `icu_normalizer` for string normalization, and ICU4X for internationalization. These are standard/only choices in the Rust ecosystem.
- **One shared error message**: "Keyword must not contain escaped characters" appears in both parsers. This is a near-universal JS engine error message derived from spec requirements.
- **Same API surface**: Both expose the same method names (`Array.prototype.map`, `String.prototype.replace`, etc.) because the spec requires them.
- **Same ICU4X sensitivity mapping**: Collator `base`=Primary, `accent`=Secondary, `case`=Primary+CaseLevel, `variant`=Tertiary. This mapping is defined in ECMA-402 Section 10.2.1.

## Notable JSSE-Unique Features

Several JSSE implementations are substantially larger or more complete than Boa's equivalents, further evidencing independent development:

- **RegExp**: 9,109 lines with a custom regex-to-Rust-regex compiler, WTF-8 byte-level matching for `\p{Cs}`/`\p{Co}`, PUA-to-surrogate mapping. Boa delegates to the `regress` crate (2,137 lines).
- **Temporal**: 23,123 lines implemented from scratch with hand-written ISO 8601 parsers and direct ICU4X calendar arithmetic. Boa wraps the `temporal_rs` library (11,713 lines).
- **JSON parser**: Fully hand-written recursive-descent JSON parser. Boa uses `serde_json` for validation then compiles JSON as JavaScript bytecode.
- **Intl**: Has `DisplayNames`, `DurationFormat`, and `RelativeTimeFormat` that Boa lacks entirely.

## Conclusion

After examining approximately 150,000+ lines of code across both codebases, covering every major subsystem (GC, lexer, parser, AST, interpreter core, Temporal, Intl, collections, iterators, generators, Proxy, Reflect, Symbol, TypedArrays, ArrayBuffer, Atomics, Array, String, Number, Math, JSON, RegExp, Promise, Date), **no evidence of code copying was found**. The implementations are independent at every level of analysis: architecture, data structures, algorithms, naming conventions, error messages, comments, code organization, and third-party dependencies.
