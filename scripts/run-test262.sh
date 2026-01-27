#!/usr/bin/env bash
# Run jsse against all test262 tests and report pass/fail/percentage.
#
# Usage:
#   ./scripts/run-test262.sh [path-to-jsse] [test262-dir]
#
# Defaults:
#   jsse binary: ./target/release/jsse
#   test262 dir: ./test262

set -euo pipefail

JSSE="${1:-./target/release/jsse}"
TEST262_DIR="${2:-./test262}"
TEST_DIR="$TEST262_DIR/test"
TIMEOUT=10

if [[ ! -x "$JSSE" ]]; then
    echo "Error: jsse binary not found at $JSSE"
    echo "Build it first: cargo build --release"
    exit 2
fi

if [[ ! -d "$TEST_DIR" ]]; then
    echo "Error: test262 test directory not found at $TEST_DIR"
    exit 2
fi

pass=0
fail=0
total=0

while IFS= read -r -d '' test_file; do
    total=$((total + 1))

    # Extract YAML frontmatter (between first and second --- markers)
    header=$(sed -n '/^---$/,/^---$/p' "$test_file" | tr -d '\0')

    is_negative=false
    if echo "$header" | grep -q "^negative:"; then
        is_negative=true
    fi

    # Run the test with timeout
    if timeout "$TIMEOUT" "$JSSE" "$test_file" >/dev/null 2>&1; then
        exit_code=0
    else
        exit_code=$?
    fi

    if $is_negative; then
        # Negative test: we expect a non-zero exit
        if [[ $exit_code -ne 0 ]]; then
            pass=$((pass + 1))
        else
            fail=$((fail + 1))
        fi
    else
        # Positive test: we expect exit code 0
        if [[ $exit_code -eq 0 ]]; then
            pass=$((pass + 1))
        else
            fail=$((fail + 1))
        fi
    fi

    # Progress indicator every 1000 tests
    if [[ $((total % 1000)) -eq 0 ]]; then
        echo "... processed $total tests" >&2
    fi
done < <(find "$TEST_DIR/language" "$TEST_DIR/built-ins" -name '*.js' -not -path '*/harness/*' -print0 2>/dev/null)

# Also count annexB tests
while IFS= read -r -d '' test_file; do
    total=$((total + 1))

    header=$(sed -n '/^---$/,/^---$/p' "$test_file" | tr -d '\0')

    is_negative=false
    if echo "$header" | grep -q "^negative:"; then
        is_negative=true
    fi

    if timeout "$TIMEOUT" "$JSSE" "$test_file" >/dev/null 2>&1; then
        exit_code=0
    else
        exit_code=$?
    fi

    if $is_negative; then
        if [[ $exit_code -ne 0 ]]; then
            pass=$((pass + 1))
        else
            fail=$((fail + 1))
        fi
    else
        if [[ $exit_code -eq 0 ]]; then
            pass=$((pass + 1))
        else
            fail=$((fail + 1))
        fi
    fi

    if [[ $((total % 1000)) -eq 0 ]]; then
        echo "... processed $total tests" >&2
    fi
done < <(find "$TEST_DIR/annexB" -name '*.js' -print0 2>/dev/null)

if [[ $total -eq 0 ]]; then
    echo "No tests found."
    exit 1
fi

percentage=$(awk "BEGIN { printf \"%.2f\", ($pass / $total) * 100 }")

echo ""
echo "=== test262 Results ==="
echo "Total:   $total"
echo "Pass:    $pass"
echo "Fail:    $fail"
echo "Rate:    ${percentage}%"
echo ""
echo "JSON: {\"total\": $total, \"pass\": $pass, \"fail\": $fail, \"percentage\": $percentage}"
