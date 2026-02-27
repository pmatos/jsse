# Third-Party JS Test Suite Candidates for JSSE

## Context

JSSE already runs the **acorn** parser test suite (13,507/13,507 tests passing) using a
bundle-and-shim approach:
1. Clone the project, build it
2. Bundle tests with esbuild (`--format=iife --platform=neutral`) into a single file
3. Prepend minimal shims for missing Node.js globals (`process`, `console.group`, etc.)
4. Run the final bundle on `jsse`

This document evaluates additional JS projects whose test suites could showcase JSSE
using the same approach. We want projects that are:
- **Well-known** (impressive to showcase)
- **Pure ECMAScript** (no Node.js APIs like `fs`/`path`/`child_process`, no DOM)
- **Bundleable** with esbuild into a single IIFE
- **Substantial** test count (hundreds to thousands)
- **Self-contained** (no network, no filesystem, no browser-specific APIs)

---

## Recommended Candidates

### 1. Underscore.js (Top Pick)

| Attribute | Detail |
|---|---|
| **GitHub** | [jashkenas/underscore](https://github.com/jashkenas/underscore) |
| **Stars** | ~27k |
| **Test Framework** | QUnit 2.10.1 |
| **Test Count** | ~600+ tests, ~1,500+ assertions |
| **Runtime Dependencies** | Zero |
| **Node.js APIs in tests** | None (designed to run in browser at underscorejs.org/test/) |
| **License** | MIT |

**Why it's ideal:**
- Tests use QUnit and are explicitly designed to run in a browser — the test suite is
  hosted at [underscorejs.org/test/](https://underscorejs.org/test/) as a browser page.
- The library has **zero runtime dependencies**.
- Test files are organized by category: `arrays.js`, `collections.js`, `functions.js`,
  `objects.js`, `utility.js`, `chaining.js` — each wrapped in an IIFE with conditional
  `require` (falls back to `window._`).
- Tests exercise a **broad range of JS features**: closures, higher-order functions,
  iterators, regex, prototype chains, `arguments` object, `this` binding, deep equality,
  type checking, template compilation, and more.
- **Different domain** from acorn (utility library vs parser) — demonstrates JSSE's
  breadth.

**Setup approach:**
1. Clone underscore, `npm install`, build
2. Bundle QUnit + underscore + all test files with esbuild into a single IIFE
3. Shim: `QUnit` setup (QUnit already runs in browsers, so it should work with
   minimal shims — mainly need to provide output hooks since there's no DOM)
4. Potentially use QUnit's CLI mode or create a lightweight QUnit shim that maps
   `QUnit.test` / `assert.*` to console output

**Estimated shimming work:** Moderate. QUnit is a browser-native test framework, but it
expects DOM elements for output. A shim would redirect QUnit's reporting to console.
Alternatively, QUnit has a CLI mode via `qunit` npm package that works without DOM.

**Risk factors:**
- `cross-document.js` test file may test iframe/cross-window scenarios (skip it)
- `_.template()` tests compile JS from strings (requires `Function()` constructor support)
- Some tests may reference `window` or `document` for edge cases

---

### 2. Esprima (Top Pick)

| Attribute | Detail |
|---|---|
| **GitHub** | [jquery/esprima](https://github.com/jquery/esprima) |
| **Stars** | ~7.1k |
| **Test Framework** | Custom fixture-based runner |
| **Test Count** | ~1,600 unit tests |
| **Runtime Dependencies** | Zero |
| **Node.js APIs in tests** | `fs` for reading fixture files (needs inlining) |
| **License** | BSD-2-Clause |

**Why it's ideal:**
- Esprima is another well-known JS parser (same domain as acorn) — it's an
  **industry-standard** ECMAScript parser used by thousands of projects.
- The library itself is **pure JavaScript with zero runtime dependencies**.
- esbuild's own README has historically confirmed it can bundle esprima successfully.
- The test suite is fixture-based: pairs of JS source code and expected AST/token output.
  This format is inherently pure computation — no I/O needed once fixtures are inlined.
- ~1,600 tests provide substantial coverage of ECMAScript parsing.

**Setup approach:**
1. Clone esprima, `npm install`, build (TypeScript → JS)
2. Inline all test fixtures (`.tree.json`, `.tokens.json`, `.failure.json`) into the
   test runner code, replacing `fs.readFileSync` calls
3. Bundle with esbuild into a single IIFE
4. Shim: `process.exit()`, possibly `console.error` formatting

**Estimated shimming work:** Moderate. The main challenge is that the test runner reads
fixture files from disk with `fs`. A pre-processing script (similar to
`patch-acorn-comments.js`) could inline all fixtures into the test runner code before
bundling. The actual test logic is pure JS comparison.

**Risk factors:**
- Fixture inlining requires a pre-processing script to replace `fs.readFileSync` calls
- `hostile-environment-tests.js` may test unusual global state scenarios
- Some tests may rely on `process.argv` or similar Node.js globals

---

## Other Strong Candidates

### 3. Validator.js

| Attribute | Detail |
|---|---|
| **GitHub** | [validatorjs/validator.js](https://github.com/validatorjs/validator.js) |
| **Stars** | ~23.8k |
| **Test Framework** | Mocha |
| **Test Count** | Hundreds (comprehensive string validation tests) |
| **Runtime Dependencies** | Zero |
| **Node.js APIs in tests** | Minimal — pure string validation |
| **License** | MIT |

**Why it's good:**
- Pure string validation library with zero dependencies — all operations are pure
  ECMAScript string/regex manipulation.
- Tests are Mocha-based with simple `describe`/`it`/`assert` patterns.
- Very well-known (23.8k stars, 19M+ weekly npm downloads).
- Tests exercise: string manipulation, regex matching, type coercion, Unicode handling,
  edge cases in format validation (email, URL, IP, UUID, etc.).
- Has a `clientSide.test.js` confirming browser compatibility.

**Setup approach:**
1. Clone validator.js, `npm install`, build
2. Bundle all test files + Mocha runtime with esbuild
3. Shim: Mocha `describe`/`it`/`assert` interface → console output

**Estimated shimming work:** Moderate. Need a lightweight Mocha shim or bundle Mocha's
browser-compatible runtime. The `describe`/`it` pattern is straightforward to shim.

**Risk factors:**
- Some validator functions (like `isLocale`, `isMobilePhone`) have large lookup tables
  that might stress memory
- `timezone-mock` dev dependency suggests some tests mock timezone behavior

---

### 4. Ramda

| Attribute | Detail |
|---|---|
| **GitHub** | [ramda/ramda](https://github.com/ramda/ramda) |
| **Stars** | ~24.1k |
| **Test Framework** | Mocha (+ Testem for browser testing) |
| **Test Count** | ~1,000+ (200+ test files, one per function) |
| **Runtime Dependencies** | Zero |
| **Node.js APIs in tests** | None — pure functional operations |
| **License** | MIT |

**Why it's good:**
- Pure functional programming library — all functions are side-effect free, operating
  on plain JS values (arrays, objects, strings, numbers).
- Tests are designed to run in browsers via Testem.
- Exercises: currying, function composition, lens operations, immutable data
  manipulation, transducers, algebraic data types — stress-tests closures, higher-order
  functions, and prototype chains extensively.
- Well-known in the FP community (24k stars).

**Setup approach:**
1. Clone ramda, `npm install`, build
2. Bundle all test files with esbuild (resolves ES module imports)
3. Shim: Mocha `describe`/`it`/`assert` + `assert` module equivalents

**Estimated shimming work:** Low-moderate. Tests use Mocha + Node's `assert` module.
A simple shim for `assert.strictEqual`, `assert.deepEqual`, `assert.throws` etc. would
suffice since Ramda's tests don't use any Node.js APIs.

**Risk factors:**
- Test files use ES module imports (`import ... from '../source/...'`) — esbuild handles
  this natively
- Some tests may use `Symbol.iterator` or other ES6+ features extensively

---

## Candidates Evaluated but Not Recommended

### Lodash
- ~60k stars, ~5,000 tests — impressive numbers but the test infrastructure is complex
  and entangled with Node.js paths. The v4→v5 transition has left the test setup in flux.
  Underscore.js covers the same domain with cleaner, browser-native tests.

### Babel Parser (@babel/parser)
- Excellent parser with thousands of fixture tests, but deeply embedded in Babel's
  monorepo. The fixture-based tests read from `fs`, and extracting them from the
  monorepo's Jest/lerna infrastructure would be extremely complex.

### Prettier
- 51k+ stars, but tests are snapshot-based (read fixture → format → compare to stored
  snapshot). Fundamentally tied to the filesystem. Not feasible to bundle.

### Day.js
- 48k+ stars, pure JS date library, but tests use Jest (heavier to shim than
  Mocha/QUnit) and some tests compare against Moment.js. Timezone tests depend on
  `TZ` environment variable.

### Math.js
- 14.7k stars, ~4,500 tests, but has multiple runtime dependencies (typed-function,
  complex.js, fraction.js, decimal.js). The dependency injection system and complex
  module structure make bundling non-trivial.

### Peggy (PEG.js successor)
- Parser generator (good domain fit), but uses `node --experimental-vm-modules` and
  likely reads grammar files from disk. Only ~1.1k stars.

### Chevrotain
- Parser toolkit with ~2.8k stars. Tests are well-structured but the library uses
  TypeScript classes extensively and the test setup is less straightforward.

---

## Comparison Matrix

| Project | Stars | Tests | Framework | Node APIs | Bundle Ease | Domain |
|---|---|---|---|---|---|---|
| **Underscore.js** | 27k | ~600+ | QUnit | None | Easy | Utility |
| **Esprima** | 7.1k | ~1,600 | Custom | `fs` (fixtures) | Moderate | Parser |
| **Validator.js** | 23.8k | Hundreds | Mocha | None | Easy | Validation |
| **Ramda** | 24.1k | ~1,000+ | Mocha | None | Easy | Functional |
| Lodash | 60k | ~5,000 | Custom/QUnit | Some | Hard | Utility |
| Day.js | 48.6k | Hundreds | Jest | Some | Moderate | Date/Time |

---

## Recommendation

**Pick two from the top four:**

1. **Underscore.js** — Best overall candidate. Browser-native QUnit tests, zero deps,
   well-known, covers a completely different domain (utility functions) from acorn.
   Demonstrates JSSE's breadth beyond just parsers.

2. **Esprima** — Proven bundleable, same domain as acorn but different parser (shows
   JSSE can run multiple real-world parsers). ~1,600 tests is the largest count among
   the easy candidates.

**Alternative pairing:** Underscore.js + Ramda (two different utility/FP libraries) or
Underscore.js + Validator.js (utility + validation) for maximum domain diversity.

The ideal showcase combination alongside acorn would be:
- **Acorn** (parser, 13,507 tests) — already done
- **Underscore.js** (utility library, ~600+ tests) — different domain, browser-native tests
- **Esprima** or **Validator.js** (parser or validation, ~1,600 or hundreds of tests)

This demonstrates that JSSE can run substantial, real-world JavaScript applications
across multiple domains — not just parsers.
