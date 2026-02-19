# Plan: Running Acorn's Test Suite on JSSE

## Goal

Run acorn's full test suite (~3,500 test cases x 4 modes = ~12,000 invocations) on
jsse as a real-world integration test. Acorn is a JavaScript parser written in
JavaScript — if jsse can run it, that proves the engine handles real-world JS.

## The Problem: CommonJS

Acorn's test suite uses CommonJS (`require()` / `exports`). jsse does not
implement CommonJS. The test entry point (`test/run.js`) does 35 `require()`
calls. Every test file uses `exports` to register test cases into a shared array.

We will **not** implement CommonJS in jsse. Instead, we bundle the entire test
suite into a single file with no `require()` calls using esbuild.

---

## Architecture of Acorn's Test Suite

```
test/run.js          ← entry point (IIFE)
  ├── require("./driver.js")        ← test framework: exports.test(), exports.runTests()
  ├── require("./tests.js")         ← 687 test cases (each calls test/testFail)
  ├── require("./tests-harmony.js") ← 619 test cases
  ├── ... 31 more test-*.js files   ← ~2,200 more test cases
  ├── require("../acorn")           ← acorn parser library (CJS dist)
  └── require("../acorn-loose")     ← acorn loose parser (CJS dist)
```

**How it works:**

1. `driver.js` exports `test()`, `testFail()`, `testAssert()` — each pushes a
   test descriptor onto a module-scoped `tests[]` array.
2. Each `tests-*.js` file calls `test(code, expectedAST, options)` hundreds of
   times, building up the test array.
3. `run.js` calls `driver.runTests(config, report)` which iterates the array,
   parses each code string with `acorn.parse()`, and compares the AST output
   to the expected object via deep comparison (`misMatch()`).
4. This runs in 4 modes: Normal, Loose, Normal+commonjs, Loose+commonjs.

**Key insight:** The test files themselves are almost entirely ES5. The only
ES6+ at the test-harness level is:
- 2 arrow functions in `run.js` (the commonjs mode `parse:` configs)
- `Object.assign` in `run.js` (2 uses)
- `const`/`let`/arrow/template literals in `tests-bigint.js`

The acorn library source uses ES6+ classes, arrow functions, `let`/`const`,
destructuring, template literals, etc. — but Rollup + buble transpiles this
to ES5 in the CJS dist (`acorn/dist/acorn.js`). So the **actually-executed**
acorn code is ES5-ish.

---

## Step-by-Step Procedure

### Phase 1: Setup and Bundle Creation

#### Step 1: Clone acorn and build it

```bash
cd /tmp
git clone --depth 1 https://github.com/acornjs/acorn.git
cd acorn
npm install
npm run build    # Rollup builds acorn/dist/acorn.js and acorn-loose/dist/acorn-loose.js
```

The Rollup build uses the `buble` plugin which transpiles ES6+ → ES5
(classes → functions, arrows → functions, template literals → concatenation).
This means the dist files are already ES5-friendly.

#### Step 2: Bundle everything into one file with esbuild

```bash
npx esbuild test/run.js \
  --bundle \
  --format=iife \
  --platform=neutral \
  --outfile=acorn-tests-bundle.js
```

Key flags:
- `--bundle`: resolve all `require()` calls and inline everything
- `--format=iife`: wrap output in a self-executing function (no CJS/ESM)
- `--platform=neutral`: don't inject Node.js polyfills

This produces a **single self-contained .js file** with zero `require()` calls.

#### Step 3: Add runtime shims

The bundled file still references `process` and `console.group`. Prepend shims:

```javascript
// Minimal shims for acorn test runner
if (typeof console === "undefined") {
  globalThis.console = { log: function() {} };
}
if (typeof console.group === "undefined") {
  console.group = function(name) { console.log("--- " + name + " ---"); };
}
if (typeof console.groupEnd === "undefined") {
  console.groupEnd = function() {};
}
if (typeof process === "undefined") {
  globalThis.process = {
    exit: function(code) {
      if (code !== 0) throw new Error("Process exit with code " + code);
    },
    stdout: { write: function(s, cb) { if (cb) cb(); } }
  };
}
```

Alternative: create `acorn-shim.js` with the shims and concatenate:
```bash
cat acorn-shim.js acorn-tests-bundle.js > acorn-tests-final.js
```

#### Step 4: Run on jsse

```bash
cargo build --release
./target/release/jsse acorn-tests-final.js
```

### Phase 2: Feature Audit

Before running, systematically verify that jsse supports every JS feature the
bundled code actually uses at runtime. The buble transpilation means most ES6+
is eliminated, but some remains.

#### Features used by the test harness (run.js + driver.js + test files)

| Feature | Used Where | jsse Status | Notes |
|---------|-----------|-------------|-------|
| `var`, `function`, closures, IIFE | Everywhere | ✅ | Core ES5 |
| `for` / `for-in` loops | driver.js, run.js | ✅ | |
| `try/catch` | driver.js | ✅ | |
| `instanceof SyntaxError` | driver.js | ✅ | |
| `instanceof RegExp` | driver.js (misMatch) | ✅ | |
| `JSON.stringify` | driver.js (ppJSON) | ✅ | |
| `typeof` checks | run.js, tests.js | ✅ | |
| `.charAt()`, `.indexOf()`, `.slice()` | driver.js | ✅ | |
| `Array.push`, `.length`, `.splice` | driver.js | ✅ | |
| `console.log` | run.js | ✅ | |
| `console.group` / `console.groupEnd` | run.js | ❓ Check | Need shim or impl |
| `+new Date` (timing) | run.js | ✅ | |
| `Object.assign` | run.js (2 uses) | ✅ | |
| Arrow functions (`=>`) | run.js (2 uses) | ✅ | |
| `process.exit` / `process.stdout.write` | run.js | ❌ Need shim | Not a JS built-in |
| `typeof BigInt` guard | tests-bigint.js | ✅ | |
| `BigInt()` constructor | tests-bigint.js | ✅ | |
| `const` / `let` | tests-bigint.js | ✅ | |
| Template literals | tests-bigint.js | ✅ | |
| `document.getElementById` | run.js (browser path) | N/A | Guarded by typeof, never executes |

#### Features used by acorn's parser (dist/acorn.js — buble-transpiled)

Since buble transpiles classes→functions, arrows→functions, template
literals→concatenation, the dist is mostly ES5. But buble does NOT transpile:

| Feature | Used Where | jsse Status | Notes |
|---------|-----------|-------------|-------|
| `String.fromCharCode` | identifier/tokenize | ✅ | |
| `String.fromCodePoint` | util.js (codePointToString) | ✅ Check | Surrogate pair handling |
| `.charCodeAt()` / `.codePointAt()` | Everywhere in tokenizer | ✅ | |
| `Object.create(null)` | Lookup tables | ✅ | |
| `Object.defineProperty` | Prototype setup | ✅ | |
| `Object.getPrototypeOf` | Plugin system | ✅ | |
| `parseInt` (with radix) | Number parsing | ✅ | |
| `RegExp` constructor | wordsRegexp util | ✅ | |
| `RegExp.prototype.test` | Pattern matching | ✅ | |
| `Array.isArray` | util.js fallback | ✅ | |
| `Object.hasOwn` / `hasOwnProperty` | util.js polyfill | ✅ | |
| `SyntaxError` construction | Error reporting | ✅ | |
| `RangeError` construction | Error reporting | ✅ | |
| `Error.captureStackTrace` | Optional, guarded | N/A | Guarded by typeof |
| `.toString(16)` (Number) | Unicode escapes | ✅ | |
| `String.prototype.replace` | Various | ✅ | |
| `String.prototype.match` | Various | ✅ | |
| Nested `function` (closures) | Everywhere | ✅ | |
| `arguments` object | Some functions | ✅ | |
| Getters (`get x()`) | buble may preserve | ✅ Check | Verify in dist output |

#### Features used by acorn-loose (dist/acorn-loose.js)

Same as acorn — it imports from acorn and adds loose-mode parsing. Same
buble transpilation. Key additional need:

| Feature | Notes |
|---------|-------|
| Everything in acorn | acorn-loose is an extension |
| `Array.prototype.indexOf` | Used in loose parser |
| `Array.prototype.map` | Used in loose parser |

### Phase 3: Debugging and Iteration

#### Expected failure modes (in likely order)

1. **Parse error in the bundle** — The bundled file is multi-MB. jsse's parser
   might hit edge cases with very large files, deeply nested object literals
   (the AST expectations), or unusual token patterns.

   *Debug:* If parse fails, check the error position in the bundle. Create a
   minimized repro. Fix the parser.

2. **Missing or incorrect String method** — acorn's tokenizer is string-heavy.
   A subtle bug in `charCodeAt`, `codePointAt`, `fromCodePoint`, `slice`, or
   `indexOf` could break tokenization silently.

   *Debug:* Add a trivial test that parses `acorn.parse("1 + 2")` and
   logs the result. If that works, acorn's core is functional.

3. **Memory/performance** — The bundle creates thousands of large object
   literals (the expected ASTs). The tree-walking interpreter may be slow.

   *Debug:* Time how long it takes to just load (parse) the bundle vs.
   executing. Consider running with a subset of test files first.

4. **RegExp incompatibility** — acorn uses `RegExp` for identifier matching
   (`wordsRegexp`). If jsse's RegExp has a bug with alternation patterns
   like `^(?:break|case|...|with)$`, keyword recognition breaks.

   *Debug:* Extract the `wordsRegexp` patterns and test them directly in jsse.

5. **Deep recursion in misMatch** — The `misMatch()` function in driver.js is
   recursive. Deep ASTs could blow the call stack.

   *Debug:* Check jsse's stack depth limit. May need to increase it.

6. **Object property enumeration order** — `misMatch` uses `for...in` to
   compare expected vs actual ASTs. If property enumeration order differs
   from V8, tests could report false mismatches.

   *Debug:* Check if jsse preserves insertion order per spec.

#### Incremental approach

Don't try to run all ~3,500 tests at once. Create incremental bundles:

1. **Smoke test:** Bundle only `driver.js` + `tests.js` (687 tests) + acorn.
   Skip acorn-loose entirely. Remove the Loose/commonjs modes from run.js.
   This tests: does acorn.parse() work at all? Can misMatch() compare ASTs?

2. **Normal mode only:** Add all test files, but keep only the Normal mode
   (delete Loose/commonjs modes from the bundled run.js). This avoids
   needing acorn-loose.

3. **Full suite:** Once Normal mode passes >90%, add acorn-loose and all 4 modes.

### Phase 4: Automation

Create `scripts/run-acorn-tests.sh`:

```bash
#!/bin/bash
set -e

ACORN_DIR="/tmp/acorn"
JSSE="./target/release/jsse"

# Clone and build acorn (cached)
if [ ! -d "$ACORN_DIR" ]; then
  git clone --depth 1 https://github.com/acornjs/acorn.git "$ACORN_DIR"
  cd "$ACORN_DIR"
  npm install
  npm run build
  cd -
fi

# Bundle
cd "$ACORN_DIR"
npx esbuild test/run.js \
  --bundle \
  --format=iife \
  --platform=neutral \
  --outfile=/tmp/acorn-tests-bundle.js

# Prepend shims
cat > /tmp/acorn-shim.js << 'SHIM'
if (typeof console === "undefined") {
  globalThis.console = { log: function() {} };
}
if (typeof console.group === "undefined") {
  console.group = function(name) { console.log("--- " + name + " ---"); };
}
if (typeof console.groupEnd === "undefined") {
  console.groupEnd = function() {};
}
if (typeof process === "undefined") {
  globalThis.process = {
    exit: function(code) {
      if (code !== 0) throw new Error("Process exit with code " + code);
    },
    stdout: { write: function(s, cb) { if (cb) cb(); } }
  };
}
SHIM

cat /tmp/acorn-shim.js /tmp/acorn-tests-bundle.js > /tmp/acorn-tests-final.js
cd -

# Run
echo "Running acorn test suite on jsse..."
$JSSE /tmp/acorn-tests-final.js
```

---

## Success Criteria

| Level | Definition |
|-------|-----------|
| **Level 0** | jsse parses the bundled file without crashing |
| **Level 1** | `acorn.parse("1 + 2")` returns a valid AST |
| **Level 2** | Normal mode: >50% of tests pass |
| **Level 3** | Normal mode: >90% of tests pass |
| **Level 4** | All 4 modes: >90% of tests pass |
| **Level 5** | All 4 modes: 100% pass (jsse is fully compatible) |

---

## What This Tests That test262 Doesn't

- **Feature composition**: test262 tests features in isolation. Acorn uses
  closures + prototypes + string methods + RegExp + error handling all at once.
- **Real library patterns**: module-scoped state, factory functions, deep
  recursive comparison, large object literals.
- **Parser stress test**: jsse must parse a multi-MB file correctly.
- **Performance floor**: if jsse can't run acorn in reasonable time (<60s),
  it's too slow for real-world use.
- **Correctness of acorn's output**: if acorn runs on jsse and passes its own
  tests, then jsse's implementation of the JS features acorn uses is correct
  at a level that goes beyond individual test262 checks.

---

## Open Questions

1. **How large is the bundle?** Need to measure after building. If >5MB, parse
   time alone could be significant. Consider whether esbuild tree-shaking helps.

2. **Does buble fully eliminate getters in the dist?** If acorn's dist uses
   `get` accessors (buble doesn't transpile those), jsse needs getter support
   on object literals. Verify by grepping the built `dist/acorn.js` for `get `.

3. **BigInt test interaction**: `tests-bigint.js` creates BigInt values in test
   expectations. Does jsse's `BigInt()` constructor work as acorn expects?
   Need to verify `typeof BigInt !== "undefined"` evaluates to `true` in jsse.

4. **Loose parser code size**: acorn-loose is a separate package. If it causes
   issues, we can defer it (phases 1-2 don't need it).

5. **Node-specific test paths**: Some tests might use `require("fs")` or
   similar Node APIs. Verify the bundle has zero remaining `require()` calls
   after esbuild.
