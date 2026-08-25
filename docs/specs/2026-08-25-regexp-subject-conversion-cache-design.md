# RegExp subject conversion cache design

## Context

`RegExpBuiltinExec` operates on an ECMAScript String and reports match indices in
UTF-16 code units. JSSE stores that String as an immutable `Arc<Vec<u16>>`, while
its regex backends consume UTF-8, PUA-mapped UTF-8, or WTF-8. The current exec
path rebuilds those representations from the full subject on every call, clones
the complete UTF-16 vector, and eagerly copies the Annex B input and left/right
contexts after every successful match. Repeated sticky or global exec calls on
one large subject consequently do O(subject length) work for every token.

A release-mode sticky scan confirms the scaling: 5,000, 10,000, and 20,000 ASCII
tokens take approximately 0.12, 0.42, and 1.49 seconds respectively. Doubling
the subject therefore costs 3.4-3.6 times as much, close to quadratic rather
than linear growth.

The specification's `RegExpBuiltinExec` algorithm treats the subject as an
immutable String, uses its UTF-16 length and code-unit indices, and installs the
original String as the result array's `input` property. This permits JSSE to
retain the original `JsString` and cache backend-only representations without
changing observable values.

## Approaches considered

1. Add a single-entry cache to the existing regexp-owned interpreter state,
   keyed by the `Arc<Vec<u16>>` identity of the subject. Cache the backend input
   forms and carry the original `JsString` through exec. This directly targets
   tokenizer workloads, bounds retained memory to one subject, and does not
   change the engine-wide string representation.
2. Change `JsString` to wrap a new allocation containing UTF-16 plus lazy regex
   representations. This would share conversions across interpreters and cache
   churn, but it expands every string allocation and makes a regex optimization
   part of the core value representation.
3. Use a content-keyed LRU of converted subjects. This handles alternating
   equal strings, but hashing or comparing a large subject on every lookup is
   itself O(n), so it cannot remove the tokenizer scan's quadratic work.

JSSE will use approach 1. String identity is an implementation cache key only;
ECMAScript String equality remains value-based. A cache miss for a distinct but
equal backing allocation is acceptable because avoiding that miss would require
the full-subject operation this change removes.

## Design

Introduce a private `RegexInput` owned through `Rc`. It retains the original
`JsString`, eagerly creates the normal Unicode regex string, and lazily creates
the non-Unicode PUA form and WTF-8 byte form when a backend needs them. The
single-entry cache retains the most recently used `RegexInput`; a cloned
`JsValue` for the same String has the same backing `Arc` and returns the same
`Rc` without traversing or copying its code units.

Refactor the internal regexp execution seam to accept `RegexInput` rather than
an independently allocated Rust `String` and `Vec<u16>`. The exec path selects
the cached representation after re-reading flags, uses the original UTF-16
length directly, and puts a clone of the original `JsString` in the match result.
Custom `exec` calls likewise receive that original String. Existing regexp
symbol methods use the same object, so they retain their current coercion order
while sharing the conversion.

On a successful match, store the original `JsString` and the match's UTF-16
start/end in the Annex B state. `RegExp.input` returns the retained String;
`leftContext` and `rightContext` copy only their requested UTF-16 ranges when
their accessors run. Setting `RegExp.input` replaces only that property and does
not disturb the context subject or offsets from the last match. Match text and
capture statics remain eager because their size is bounded by match/capture
content rather than necessarily duplicating the full subject.

The UTF-16-to-regex conversion will reserve capacity. Pure ASCII subjects take
an exact-capacity byte conversion fast path, and the non-Unicode form aliases
the Unicode form instead of performing another pass. `RegexInput` also retains
the ASCII classification so byte/UTF-16 offset conversion does not rediscover
it by scanning the complete subject on every exec.

Compiled-pattern cache key redesign and non-ASCII byte-offset maps are excluded
from this slice. Pattern keys are normally small and do not account for the
full-subject O(n) passes. Offset mapping is a separate representation question
that is unnecessary for the reported ASCII tokenizer workload.

## Correctness and failure behavior

The cache holds only immutable `JsString` storage, so there is no invalidation
on JavaScript-visible mutation. Identity matching uses `Arc::ptr_eq` while the
cached strong reference prevents allocator-address reuse. Conversion allocation
failures have the same process-level out-of-memory behavior as the existing
eager allocations. User-code coercion and `lastIndex` side effects still happen
before matching in their existing order.

UTF-16 offsets, rather than backend byte offsets, are retained for Annex B
contexts. This preserves lone surrogates and makes context materialization
independent of which regex backend produced the match.

Each lazy form is built from the retained UTF-16 code units, never derived from
another form. The PUA encoding of lone surrogates is not injective: U+F0000 is a
real assignable code point, so a genuine plane-15 scalar in U+F0000-U+F07FF is
indistinguishable in the Unicode form from an encoded lone surrogate, and
deriving the non-Unicode form from it silently loses one code unit of the
subject. Aliasing is therefore confined to pure-ASCII subjects, where no
surrogate encoding is in play. The residual divergences that the ambiguous
encoding itself causes are tracked separately and are unchanged by this slice.

## Validation

- Add unit coverage proving repeated use of the same `JsString` returns the
  identical cached `RegexInput`, while a distinct backing allocation misses.
- Add semantic coverage for lazy Annex B input/contexts, including an input
  setter after a match and lone-surrogate slicing.
- Re-run the release sticky-scan measurement and require a doubling ratio near
  linear, with a large absolute improvement over the recorded baseline.
- Run custom tests, the official built-in RegExp and Annex B RegExp test262
  directories, formatting, clippy, release build/tests, and the full test262
  suite without updating the feature-branch baseline.
