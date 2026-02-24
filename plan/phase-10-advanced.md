# Phase 10: Advanced Features

**Spec Reference:** §17 (Error Handling), §29 (Memory Model), Annex B

## Goal
Implement error handling extensions, the memory model for shared memory, and all Annex B legacy web compatibility features.

## Tasks

### 10.1 Error Handling & Language Extensions (§17)
- [ ] Forbidden extensions list
- [ ] Implementation-defined behavior audit
- [ ] Host hook implementations

### 10.2 Memory Model (§29)
- [ ] Memory model for SharedArrayBuffer
- [ ] Agent event records
- [ ] Shared data block events
- [ ] ReadSharedMemory / WriteSharedMemory
- [ ] Races and data races
- [ ] Sequentially consistent atomics
- [ ] Valid executions
- [ ] Memory order (happens-before, synchronizes-with)
- [ ] Tear-free reads

### 10.3 Annex B: Additional ECMAScript Features for Web Browsers (§B)
- [ ] **B.1 Additional Syntax**
  - [ ] B.1.1 HTML-like comments (`<!--`, `-->`)
  - [ ] B.1.2 Regular expression patterns (legacy quantifier, octal escapes, identity escapes)
  - [ ] B.1.3 Legacy octal and octal-like numeric literals
  - [ ] B.1.4 Legacy string escape sequences
- [x] **B.2 Additional Built-in Properties (partial)**
  - [x] `escape()` and `unescape()` global functions
  - [x] `Object.prototype.__proto__`
  - [x] `Object.prototype.__defineGetter__`, `__defineSetter__`, `__lookupGetter__`, `__lookupSetter__`
  - [x] `String.prototype` HTML methods + `substr()`
  - [x] `Date.prototype.getYear()`, `.setYear()`, `.toGMTString()`
  - [ ] `RegExp` legacy static properties (`RegExp.$1`–`$9`, `RegExp.input`, `RegExp.lastMatch`, etc.)
- [x] **B.3 Other Additional Features (partial)**
  - [x] B.3.2 Block-level function declarations in sloppy mode
  - [x] B.3.3 FunctionDeclarations in IfStatement
  - [x] B.3.4 Changes to `eval` for block-level function declarations
  - [ ] B.3.5 `for-in` initializer (deprecated)
  - [ ] B.3.6 `arguments` and eval in parameter initializers
  - [ ] Changes to IsHTMLDDA (`[[IsHTMLDDA]]` internal slot: `typeof === "undefined"`, falsy)

### 10.4 Intl (ECMA-402)
- [x] `Intl` namespace
- [x] `Intl.DateTimeFormat` (partial — dateStyle/timeStyle, calendar-aware formatting)
- [x] `Intl.NumberFormat` (partial)
- [x] **intl402/Temporal: 3,838/3,838 (100.00%)** — full Temporal internationalization support
- [ ] `Intl.Collator`
- [ ] `Intl.DisplayNames`
- [ ] `Intl.DurationFormat`
- [ ] `Intl.ListFormat`
- [ ] `Intl.Locale`
- [ ] `Intl.PluralRules`
- [ ] `Intl.RelativeTimeFormat`
- [ ] `Intl.Segmenter`

### 10.5 Optimization & Hardening
- [ ] Inline caching for property access
- [ ] Hidden classes / shapes
- [ ] String interning
- [ ] Garbage collection (mark-sweep initially)
- [ ] Stack overflow protection
- [ ] Interrupt/timeout support

## test262 Tests
- `test262/test/annexB/` — 1,377 scenarios (1,264 passing, 91.8%)
- `test262/test/intl402/Temporal/` — 3,838 scenarios (3,838 passing, 100.00%)
- `test262/test/intl402/` (non-Temporal) — varies (optional)
- `test262/test/staging/` — experimental/proposal tests (optional)
