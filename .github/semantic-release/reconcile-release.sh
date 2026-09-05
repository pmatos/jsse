#!/usr/bin/env bash
# Reconciles the latest vX.Y.Z release tag against its GitHub Release.
#
# semantic-release decides "is there anything new to release" purely from git
# tags, not GitHub Release state: once a tag is pushed, semantic-release never
# revisits it. If @semantic-release/github's asset upload (or the draft->
# published PATCH that follows it) fails after the tag has already landed,
# reruns of this workflow just cut the *next* version and the broken tag is
# orphaned with no release, or a draft release, or a release missing assets.
#
# This script runs before semantic-release on every invocation of this
# workflow and repairs that gap: it finds the latest vX.Y.Z tag regardless of
# where HEAD currently is, and if its GitHub Release is missing, still a
# draft, or missing an expected asset, rebuilds the release artifacts via
# prepare.sh from a detached worktree at that tag and republishes against the
# *existing* tag with `gh release create`/`upload --clobber`/`edit`. See #347.
#
# No arguments; discovers the tag to reconcile itself. PREPARE_SCRIPT (env,
# defaults to prepare.sh next to this script) is overridable for testing.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(git rev-parse --show-toplevel)"
PREPARE_SCRIPT="${PREPARE_SCRIPT:-$SCRIPT_DIR/prepare.sh}"
TARGET="x86_64-unknown-linux-gnu"

cd "$REPO_ROOT"

TAG="$(git tag -l | grep -E '^v[0-9]+\.[0-9]+\.[0-9]+$' | sort -V | tail -n1 || true)"
if [ -z "$TAG" ]; then
    echo "reconcile-release: no vX.Y.Z tags found; nothing to reconcile"
    exit 0
fi

VERSION="${TAG#v}"
STAGE="jsse-${TAG}-${TARGET}"
TARBALL="${STAGE}.tar.gz"
CHECKSUMS="SHA256SUMS.txt"

echo "reconcile-release: latest release tag is ${TAG}"

VIEW_JSON=""
RELEASE_FOUND=0
if VIEW_JSON="$(gh release view "$TAG" --json isDraft,assets 2>/dev/null)"; then
    RELEASE_FOUND=1
    IS_DRAFT="$(jq -r '.isDraft' <<<"$VIEW_JSON")"
    HAS_ALL_ASSETS=1
    for name in "$TARBALL" "$CHECKSUMS"; do
        if ! jq -e --arg n "$name" '.assets[]? | select(.name == $n)' <<<"$VIEW_JSON" >/dev/null; then
            HAS_ALL_ASSETS=0
        fi
    done

    if [ "$HAS_ALL_ASSETS" -eq 1 ] && [ "$IS_DRAFT" = "false" ]; then
        echo "reconcile-release: ${TAG} already published with all assets; nothing to reconcile"
        exit 0
    fi
fi

WORKTREE="$(mktemp -d)"
rmdir "$WORKTREE"
cleanup() {
    git worktree remove --force "$WORKTREE" >/dev/null 2>&1 || true
    rm -rf "$WORKTREE"
}
trap cleanup EXIT

git worktree add --detach "$WORKTREE" "$TAG" >/dev/null
(cd "$WORKTREE" && "$PREPARE_SCRIPT" "$VERSION")

if [ "$RELEASE_FOUND" -eq 0 ]; then
    echo "reconcile-release: ${TAG} has no GitHub Release; creating one"
    NOTES="$(git -C "$WORKTREE" log -1 --format=%b "$TAG")"
    gh release create "$TAG" \
        "$WORKTREE/release-upload/${TARBALL}" \
        "$WORKTREE/release-upload/${CHECKSUMS}" \
        --title "$TAG" \
        --notes "$NOTES"
else
    echo "reconcile-release: ${TAG} is missing an expected asset; uploading"
    gh release upload "$TAG" \
        "$WORKTREE/release-upload/${TARBALL}" \
        "$WORKTREE/release-upload/${CHECKSUMS}" \
        --clobber
    if [ "$IS_DRAFT" = "true" ]; then
        echo "reconcile-release: ${TAG} is still a draft; publishing"
        gh release edit "$TAG" --draft=false
    fi
fi
