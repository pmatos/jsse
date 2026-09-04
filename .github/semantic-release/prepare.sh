#!/usr/bin/env bash
# Invoked by semantic-release (@semantic-release/exec prepareCmd) with the next
# version. Syncs the crate version, builds the release binary, and stages the
# release tarball + checksums under release-upload/ for @semantic-release/github.
set -euo pipefail
VERSION="${1:?usage: prepare.sh <version>}"
TAG="v${VERSION}"
TARGET="x86_64-unknown-linux-gnu"

CURRENT="$(cargo metadata --no-deps --format-version 1 | jq -r '.packages[0].version')"
sed -i "0,/^version = \"${CURRENT}\"/s//version = \"${VERSION}\"/" Cargo.toml
cargo update -p jsse

cargo build --release --locked

STAGE="jsse-${TAG}-${TARGET}"
rm -rf release-upload
mkdir -p "release-upload/${STAGE}"
strip target/release/jsse
cp target/release/jsse README.md LICENSE "release-upload/${STAGE}/"
tar -czf "release-upload/${STAGE}.tar.gz" -C release-upload "${STAGE}"
rm -rf "release-upload/${STAGE:?}"
( cd release-upload && sha256sum -- *.tar.gz > SHA256SUMS.txt )
ls -la release-upload
