#!/usr/bin/env bash
# Pre-PR verification: build jsse, run a test262 subset and the differential
# check against node, then print one report. The `verify` skill
# (.claude/skills/verify/SKILL.md) drives this before an agent opens a PR.
#
#   ./scripts/verify.sh                          # default subset
#   ./scripts/verify.sh test262/test/built-ins/Number/   # plus the area you touched
#   ./scripts/verify.sh --full                   # also clippy + unit tests
#   ./scripts/verify.sh --shipped                # fat-LTO build instead of the quick one
#
# Steps keep running after a failure so the report shows everything at once.
# The report is also written to target/verify/report.md for the PR body.
# Exit status: 0 when every blocking step passed. The differential step is
# observe-only for now: its divergences are reported but never fail the run.
set -uo pipefail

cd "$(dirname "$0")/.."

full=0
shipped=0
sample_rate="${VERIFY_TEST262_SAMPLE:-0.02}"
sample_seed="${VERIFY_TEST262_SEED:-262023}"
extra_paths=()
for arg in "$@"; do
    case "$arg" in
        --full) full=1 ;;
        --shipped) shipped=1 ;;
        -h | --help)
            sed -n '2,15p' "$0" | sed 's/^# \{0,1\}//'
            exit 0
            ;;
        -*)
            echo "verify.sh: unknown option $arg" >&2
            exit 2
            ;;
        *) extra_paths+=("$arg") ;;
    esac
done

out_dir=target/verify
mkdir -p "$out_dir"
report="$out_dir/report.md"
rows=()
blocking_failed=0

# run_step NAME BLOCKING LOG COMMAND...
run_step() {
    local name="$1" blocking="$2" log="$out_dir/$3"
    shift 3
    echo "==> $name" >&2
    local start=$SECONDS
    "$@" >"$log" 2>&1
    local rc=$?
    local secs=$((SECONDS - start)) status
    if [ "$rc" -eq 0 ]; then
        status=pass
    elif [ "$blocking" = yes ]; then
        status="FAIL (exit $rc)"
        blocking_failed=1
    else
        status="diverged (observe-only, exit $rc)"
    fi
    echo "    $status in ${secs}s, log: $log" >&2
    rows+=("| $name | $status | ${secs}s | \`$log\` |")
    return "$rc"
}

run_step "cargo fmt --check" yes fmt.log cargo fmt --check

if [ "$full" -eq 1 ]; then
    run_step "clippy" yes clippy.log cargo clippy --all-targets -- -D warnings
fi

if [ "$shipped" -eq 1 ]; then
    build_cmd=(cargo build --release)
else
    # The quick configuration from AGENTS.md "Building": same semantics, no
    # fat LTO. Do not use the resulting binary for timing comparisons.
    build_cmd=(env CARGO_PROFILE_RELEASE_LTO=off CARGO_PROFILE_RELEASE_CODEGEN_UNITS=16
        cargo build --release)
fi
if ! run_step "build (release)" yes build.log "${build_cmd[@]}"; then
    echo "verify.sh: build failed, skipping the test steps" >&2
else
    if [ "$full" -eq 1 ]; then
        run_step "unit tests" yes unit.log cargo nextest run --cargo-profile release-nolto
    fi

    jsse=./target/release/jsse
    run_step "test262-extra" yes test262-extra.log \
        uv run python scripts/run-test262.py --jsse "$jsse" test262-extra/ --fail-on-failures

    if [ -d test262/test ]; then
        run_step "test262 sample (${sample_rate}, seed ${sample_seed})" yes test262-sample.log \
            uv run python scripts/run-test262.py --jsse "$jsse" \
            --sample "$sample_rate" --seed "$sample_seed"
        if [ "${#extra_paths[@]}" -gt 0 ]; then
            run_step "test262 (${extra_paths[*]})" yes test262-paths.log \
                uv run python scripts/run-test262.py --jsse "$jsse" "${extra_paths[@]}"
        fi
    else
        rows+=("| test262 | SKIPPED: submodule missing (git submodule update --init test262) | | |")
        blocking_failed=1
    fi

    run_step "differential vs node" no differential.log \
        uv run python scripts/run-differential.py --jsse "$jsse" --json "$out_dir/differential.json"
fi

{
    if [ "$blocking_failed" -eq 0 ]; then
        echo "### verify: PASS"
    else
        echo "### verify: FAIL"
    fi
    echo
    echo "Commit \`$(git rev-parse --short HEAD 2>/dev/null || echo unknown)\`$(git diff --quiet 2>/dev/null || echo ' (with uncommitted changes)')."
    echo
    echo "| step | result | time | log |"
    echo "|---|---|---|---|"
    printf '%s\n' "${rows[@]}"
    for log in test262-sample.log test262-paths.log; do
        [ -f "$out_dir/$log" ] || continue
        echo
        echo "\`$log\`:"
        echo '```'
        grep -E '^(Run|Pass|Fail|Rate): |REGRESS|^New passes' "$out_dir/$log" | head -30
        echo '```'
    done
    if [ -f "$out_dir/differential.log" ]; then
        echo
        sed -n '3,$p' "$out_dir/differential.log"
    fi
} >"$report"

cat "$report"
exit "$blocking_failed"
