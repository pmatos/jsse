# Acorn Test Suite Infrastructure — Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Create `scripts/run-acorn-tests.sh` that clones acorn, bundles its test suite into a single IIFE file, prepends runtime shims, and runs it on jsse — producing a pass/fail report.

**Architecture:** A single bash script orchestrates: clone+build acorn (cached), esbuild bundle, shim prepend, cargo build, jsse execution, output capture. All intermediate artifacts live in `/tmp`. The script is idempotent and supports `--clean` for a fresh start.

**Tech Stack:** Bash, esbuild (via npx), cargo, jsse

---

### Task 1: Create the shim file

**Files:**
- Create: `scripts/acorn-shim.js`

**Step 1: Write the shim file**

This file is prepended to the esbuild bundle to provide Node.js globals that jsse doesn't have.

```javascript
// Runtime shims for acorn test runner on jsse.
// Provides Node.js globals that the bundled test runner references.

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
    stdout: { write: function(s) {} }
  };
}
```

**Step 2: Commit**

```bash
git add scripts/acorn-shim.js
git commit -m "Add runtime shim for acorn test suite on jsse"
```

---

### Task 2: Create the run script — clone & build section

**Files:**
- Create: `scripts/run-acorn-tests.sh`

**Step 1: Write the script skeleton with clone+build logic**

```bash
#!/usr/bin/env bash
# Run acorn's test suite on jsse.
#
# Usage:
#   ./scripts/run-acorn-tests.sh [--clean]
#
# Clones acorn (cached in /tmp/acorn), bundles the test suite into a single
# IIFE file, prepends runtime shims, builds jsse, and runs the bundle.
# Use --clean to force a fresh acorn clone.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
ACORN_DIR="/tmp/acorn"
BUNDLE="/tmp/acorn-tests-bundle.js"
FINAL="/tmp/acorn-tests-final.js"
JSSE="$PROJECT_DIR/target/release/jsse"

# Handle --clean flag
if [[ "${1:-}" == "--clean" ]]; then
    echo "Cleaning cached acorn..."
    rm -rf "$ACORN_DIR" "$BUNDLE" "$FINAL"
fi

# Step 1: Clone and build acorn (cached)
if [ ! -d "$ACORN_DIR" ]; then
    echo "Cloning acorn..."
    git clone --depth 1 https://github.com/acornjs/acorn.git "$ACORN_DIR"
    echo "Installing acorn dependencies..."
    cd "$ACORN_DIR"
    npm install
    echo "Building acorn..."
    npm run build
    cd "$PROJECT_DIR"
else
    echo "Using cached acorn at $ACORN_DIR"
fi

echo "TODO: bundle, shim, build, run"
```

**Step 2: Make it executable and test the clone step**

```bash
chmod +x scripts/run-acorn-tests.sh
./scripts/run-acorn-tests.sh --clean
```

Expected: acorn cloned to `/tmp/acorn`, npm install + build succeeds, prints "TODO: bundle, shim, build, run".

**Step 3: Commit**

```bash
git add scripts/run-acorn-tests.sh
git commit -m "Add acorn test script skeleton with clone+build"
```

---

### Task 3: Add the esbuild bundle step

**Files:**
- Modify: `scripts/run-acorn-tests.sh`

**Step 1: Add the bundle step after the clone/build section**

Replace the `echo "TODO: bundle, shim, build, run"` line with:

```bash
# Step 2: Bundle test suite with esbuild
echo "Bundling acorn test suite..."
cd "$ACORN_DIR"
npx esbuild test/run.js \
    --bundle \
    --format=iife \
    --platform=neutral \
    --outfile="$BUNDLE"
cd "$PROJECT_DIR"

BUNDLE_SIZE=$(wc -c < "$BUNDLE")
echo "Bundle created: $BUNDLE ($BUNDLE_SIZE bytes)"

echo "TODO: shim, build, run"
```

**Step 2: Test the bundle step**

```bash
./scripts/run-acorn-tests.sh
```

Expected: uses cached acorn, runs esbuild, prints bundle size (likely 2-5 MB), creates `/tmp/acorn-tests-bundle.js`.

**Step 3: Verify the bundle has no require() calls**

```bash
grep -c 'require(' /tmp/acorn-tests-bundle.js || echo "0 require calls - good"
```

Expected: 0 matches (esbuild resolved all requires).

**Step 4: Commit**

```bash
git add scripts/run-acorn-tests.sh
git commit -m "Add esbuild bundle step to acorn test script"
```

---

### Task 4: Add shim prepend and jsse build steps

**Files:**
- Modify: `scripts/run-acorn-tests.sh`

**Step 1: Replace the TODO with shim prepend + cargo build**

Replace `echo "TODO: shim, build, run"` with:

```bash
# Step 3: Prepend runtime shims
echo "Prepending runtime shims..."
cat "$SCRIPT_DIR/acorn-shim.js" "$BUNDLE" > "$FINAL"

FINAL_SIZE=$(wc -c < "$FINAL")
echo "Final bundle: $FINAL ($FINAL_SIZE bytes)"

# Step 4: Build jsse
echo "Building jsse (release)..."
cd "$PROJECT_DIR"
cargo build --release

echo "TODO: run"
```

**Step 2: Test**

```bash
./scripts/run-acorn-tests.sh
```

Expected: reuses cached acorn and bundle, prepends shims, builds jsse, prints "TODO: run". Verify `/tmp/acorn-tests-final.js` starts with the shim code:

```bash
head -5 /tmp/acorn-tests-final.js
```

**Step 3: Commit**

```bash
git add scripts/run-acorn-tests.sh
git commit -m "Add shim prepend and cargo build to acorn test script"
```

---

### Task 5: Add the jsse execution and output capture

**Files:**
- Modify: `scripts/run-acorn-tests.sh`

**Step 1: Replace the final TODO with the run step**

Replace `echo "TODO: run"` with:

```bash
# Step 5: Run on jsse
echo ""
echo "========================================"
echo "  Running acorn test suite on jsse"
echo "========================================"
echo ""

EXIT_CODE=0
"$JSSE" "$FINAL" 2>&1 || EXIT_CODE=$?

echo ""
echo "========================================"
if [ $EXIT_CODE -eq 0 ]; then
    echo "  jsse exited successfully (code 0)"
else
    echo "  jsse exited with code $EXIT_CODE"
fi
echo "========================================"
```

**Step 2: Run the full pipeline**

```bash
./scripts/run-acorn-tests.sh
```

Expected: either jsse runs the acorn tests and produces output (pass/fail counts), or it crashes with a parse/runtime error. Both outcomes are informative — we capture the exit code either way.

**Step 3: Commit**

```bash
git add scripts/run-acorn-tests.sh
git commit -m "Add jsse execution step to acorn test script"
```

---

### Task 6: Verify end-to-end with Node.js as baseline

Before relying on jsse output, verify the bundle is correct by running it on Node.js.

**Step 1: Run the bundle on Node.js**

```bash
node /tmp/acorn-tests-final.js
```

Expected: Node.js runs all 4 modes and reports ~3,500 tests passing per mode with 0 failures. This confirms the bundle + shims are correct.

If Node.js shows failures, the bundle has a problem and must be fixed before testing on jsse.

**Step 2: Add a `--node` flag to the script for easy baseline comparison**

In `scripts/run-acorn-tests.sh`, after the `JSSE=...` line, add engine selection:

```bash
ENGINE="$JSSE"
if [[ "${1:-}" == "--node" ]] || [[ "${2:-}" == "--node" ]]; then
    ENGINE="node"
fi
```

And change the run step to use `$ENGINE` instead of `$JSSE`:

```bash
"$ENGINE" "$FINAL" 2>&1 || EXIT_CODE=$?
```

Also update the `--clean` check to handle both flags:

```bash
for arg in "$@"; do
    case "$arg" in
        --clean)
            echo "Cleaning cached acorn..."
            rm -rf "$ACORN_DIR" "$BUNDLE" "$FINAL"
            ;;
        --node)
            ENGINE="node"
            ;;
    esac
done
```

Move the `ENGINE` default above the arg loop.

**Step 3: Test both engines**

```bash
./scripts/run-acorn-tests.sh --node    # baseline: should pass
./scripts/run-acorn-tests.sh           # jsse: see what happens
```

**Step 4: Commit**

```bash
git add scripts/run-acorn-tests.sh
git commit -m "Add --node flag for baseline comparison in acorn test script"
```

---

### Task 7: Final cleanup and documentation

**Files:**
- Modify: `scripts/run-acorn-tests.sh` (only if minor cleanup needed)

**Step 1: Run the complete pipeline one final time**

```bash
./scripts/run-acorn-tests.sh --clean
```

Verify: clean clone, build, bundle, shim, cargo build, jsse run — all in one invocation.

**Step 2: Run with --node to confirm baseline**

```bash
./scripts/run-acorn-tests.sh --node
```

Verify: Node.js passes all tests.

**Step 3: Commit any final adjustments**

Only if something needed tweaking. Otherwise skip.

---

## Final Script Structure

After all tasks, `scripts/run-acorn-tests.sh` should:

1. Parse `--clean` and `--node` flags
2. Clone + npm install + npm run build acorn (cached in `/tmp/acorn`)
3. Bundle with `npx esbuild` into `/tmp/acorn-tests-bundle.js`
4. Cat `scripts/acorn-shim.js` + bundle into `/tmp/acorn-tests-final.js`
5. `cargo build --release` (skipped if `--node`)
6. Run the final bundle on jsse (or node with `--node`)
7. Report exit code

## Files Created/Modified

| File | Action |
|------|--------|
| `scripts/acorn-shim.js` | Created (Task 1) |
| `scripts/run-acorn-tests.sh` | Created (Task 2), modified (Tasks 3-6) |
