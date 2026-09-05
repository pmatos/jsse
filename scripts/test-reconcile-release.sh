#!/usr/bin/env bash
# Self-test for .github/semantic-release/reconcile-release.sh.
#
# Exercises each branch (no tags, fully-published, missing release, draft
# missing an asset, published missing an asset) against a throwaway git repo,
# with a stub `gh` ahead of the real one on PATH logging its invocations and
# returning canned `release view` output selected by env vars, and a stub
# PREPARE_SCRIPT that drops dummy release-upload/*.tar.gz + SHA256SUMS.txt
# instead of running a real `cargo build --release`.
#
# Usage: ./scripts/test-reconcile-release.sh

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
RECONCILE="$PROJECT_DIR/.github/semantic-release/reconcile-release.sh"

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

STUB_BIN="$WORK/bin"
mkdir -p "$STUB_BIN"

cat > "$STUB_BIN/gh" <<'STUB'
#!/usr/bin/env bash
set -euo pipefail
echo "gh $*" >> "$GH_LOG"
if [ "${1:-}" = "release" ] && [ "${2:-}" = "view" ]; then
    if [ "${GH_VIEW_EXIT:-0}" -ne 0 ]; then
        exit "${GH_VIEW_EXIT}"
    fi
    json="${GH_VIEW_JSON:-}"
    if [ -z "$json" ]; then
        json='{}'
    fi
    printf '%s' "$json"
fi
exit 0
STUB
chmod +x "$STUB_BIN/gh"

cat > "$WORK/prepare-stub.sh" <<'STUB'
#!/usr/bin/env bash
set -euo pipefail
echo "prepare $*" >> "$PREPARE_LOG"
VERSION="${1:?usage: prepare-stub.sh <version>}"
TAG="v${VERSION}"
STAGE="jsse-${TAG}-x86_64-unknown-linux-gnu"
rm -rf release-upload
mkdir -p release-upload
: > "release-upload/${STAGE}.tar.gz"
: > "release-upload/SHA256SUMS.txt"
STUB
chmod +x "$WORK/prepare-stub.sh"

FAIL=0

# make_repo <name> [extra-commits-after-tag] → sets REPO to the new repo dir,
# with an initial commit tagged v1.2.3, optionally followed by untagged
# commits (to model "the workflow reran after more commits landed on main").
make_repo() {
    local name="$1" extra="${2:-0}"
    REPO="$WORK/repos/$name"
    mkdir -p "$REPO"
    git -C "$REPO" init -q -b main
    git -C "$REPO" config user.email test@example.com
    git -C "$REPO" config user.name "Test"
    git -C "$REPO" commit -q --allow-empty -m "chore(release): 1.2.3 [skip ci]

Release notes body for v1.2.3."
    git -C "$REPO" tag v1.2.3
    local i
    for ((i = 0; i < extra; i++)); do
        git -C "$REPO" commit -q --allow-empty -m "chore: extra commit $i"
    done
}

# run_reconcile <repo> <view-exit> <view-json> → runs the script with a fresh
# log dir; sets GH_LOG/PREPARE_LOG/RC.
run_reconcile() {
    local repo="$1" view_exit="$2" view_json="$3"
    GH_LOG="$WORK/gh.log"
    PREPARE_LOG="$WORK/prepare.log"
    : > "$GH_LOG"
    : > "$PREPARE_LOG"
    RC=0
    (
        cd "$repo"
        PATH="$STUB_BIN:$PATH" \
            GH_LOG="$GH_LOG" \
            GH_VIEW_EXIT="$view_exit" \
            GH_VIEW_JSON="$view_json" \
            PREPARE_SCRIPT="$WORK/prepare-stub.sh" \
            PREPARE_LOG="$PREPARE_LOG" \
            bash "$RECONCILE"
    ) || RC=$?
}

assert_contains() {
    local haystack="$1" needle="$2" label="$3"
    if ! grep -qF -- "$needle" "$haystack"; then
        echo "FAIL $label: expected '$haystack' to contain: $needle"
        echo "--- actual $haystack ---"
        cat "$haystack"
        FAIL=1
        return 1
    fi
    return 0
}

assert_not_contains() {
    local haystack="$1" needle="$2" label="$3"
    if grep -qF -- "$needle" "$haystack"; then
        echo "FAIL $label: expected '$haystack' to NOT contain: $needle"
        echo "--- actual $haystack ---"
        cat "$haystack"
        FAIL=1
        return 1
    fi
    return 0
}

assert_empty() {
    local file="$1" label="$2"
    if [ -s "$file" ]; then
        echo "FAIL $label: expected $file to be empty"
        cat "$file"
        FAIL=1
        return 1
    fi
    return 0
}

# --- Slice 1: no tags at all → no-op ------------------------------------
test_no_tags() {
    local name="no-tags"
    REPO="$WORK/repos/$name"
    mkdir -p "$REPO"
    git -C "$REPO" init -q -b main
    git -C "$REPO" config user.email test@example.com
    git -C "$REPO" config user.name "Test"
    git -C "$REPO" commit -q --allow-empty -m "chore: initial commit"

    run_reconcile "$REPO" 0 '{}'
    if [ "$RC" -ne 0 ]; then
        echo "FAIL $name: reconcile exited $RC"
        FAIL=1
        return
    fi
    assert_empty "$GH_LOG" "$name" && echo "PASS $name"
}

# --- Slice 2: fully-published release → no-op ---------------------------
test_fully_published() {
    local name="fully-published"
    make_repo "$name" 3
    local json='{"isDraft": false, "assets": [{"name": "jsse-v1.2.3-x86_64-unknown-linux-gnu.tar.gz"}, {"name": "SHA256SUMS.txt"}]}'
    run_reconcile "$REPO" 0 "$json"
    if [ "$RC" -ne 0 ]; then
        echo "FAIL $name: reconcile exited $RC"
        cat "$GH_LOG"
        FAIL=1
        return
    fi
    assert_contains "$GH_LOG" "gh release view v1.2.3" "$name" || return
    assert_not_contains "$GH_LOG" "gh release create" "$name" || return
    assert_not_contains "$GH_LOG" "gh release upload" "$name" || return
    assert_not_contains "$GH_LOG" "gh release edit" "$name" || return
    assert_empty "$PREPARE_LOG" "$name" && echo "PASS $name"
}

# --- Slice 3: no GitHub Release yet → create path ------------------------
test_create_path() {
    local name="create-path"
    make_repo "$name" 0
    run_reconcile "$REPO" 1 ''
    if [ "$RC" -ne 0 ]; then
        echo "FAIL $name: reconcile exited $RC"
        cat "$GH_LOG"
        FAIL=1
        return
    fi
    assert_contains "$PREPARE_LOG" "prepare 1.2.3" "$name" || return
    assert_contains "$GH_LOG" "gh release create v1.2.3" "$name" || return
    assert_contains "$GH_LOG" "jsse-v1.2.3-x86_64-unknown-linux-gnu.tar.gz" "$name" || return
    assert_contains "$GH_LOG" "SHA256SUMS.txt" "$name" || return
    assert_contains "$GH_LOG" "--title v1.2.3" "$name" || return
    assert_contains "$GH_LOG" "--notes Release notes body for v1.2.3." "$name" || return
    assert_not_contains "$GH_LOG" "gh release upload" "$name" || return
    echo "PASS $name"
}

# --- Slice 4: draft release missing an asset → upload + un-draft --------
test_draft_missing_asset() {
    local name="draft-missing-asset"
    make_repo "$name" 0
    local json='{"isDraft": true, "assets": [{"name": "SHA256SUMS.txt"}]}'
    run_reconcile "$REPO" 0 "$json"
    if [ "$RC" -ne 0 ]; then
        echo "FAIL $name: reconcile exited $RC"
        cat "$GH_LOG"
        FAIL=1
        return
    fi
    assert_contains "$PREPARE_LOG" "prepare 1.2.3" "$name" || return
    assert_contains "$GH_LOG" "gh release upload v1.2.3" "$name" || return
    assert_contains "$GH_LOG" "--clobber" "$name" || return
    assert_contains "$GH_LOG" "gh release edit v1.2.3 --draft=false" "$name" || return
    assert_not_contains "$GH_LOG" "gh release create" "$name" || return
    echo "PASS $name"
}

# --- Slice 5: published release missing an asset → upload, no un-draft --
test_published_missing_asset() {
    local name="published-missing-asset"
    make_repo "$name" 0
    local json='{"isDraft": false, "assets": [{"name": "SHA256SUMS.txt"}]}'
    run_reconcile "$REPO" 0 "$json"
    if [ "$RC" -ne 0 ]; then
        echo "FAIL $name: reconcile exited $RC"
        cat "$GH_LOG"
        FAIL=1
        return
    fi
    assert_contains "$PREPARE_LOG" "prepare 1.2.3" "$name" || return
    assert_contains "$GH_LOG" "gh release upload v1.2.3" "$name" || return
    assert_not_contains "$GH_LOG" "gh release edit" "$name" || return
    assert_not_contains "$GH_LOG" "gh release create" "$name" || return
    echo "PASS $name"
}

test_no_tags
test_fully_published
test_create_path
test_draft_missing_asset
test_published_missing_asset

if [ "$FAIL" -eq 0 ]; then
    echo "OK: reconcile-release self-test green"
    exit 0
fi
echo "FAILED: reconcile-release self-test" >&2
exit 1
