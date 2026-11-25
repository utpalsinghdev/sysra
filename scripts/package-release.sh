#!/bin/sh
set -eu

target=${1:?usage: sh scripts/package-release.sh <rust-target>}
name="sysra-$target"
archive="$name.tar.gz"
staging=$(mktemp -d)
trap 'rm -rf "$staging"' EXIT

mkdir -p "$staging/$name" dist
install -m 755 "${CARGO_TARGET_DIR:-target}/$target/release/sysra" "$staging/$name/sysra"
cp LICENSE "$staging/$name/"
tar -C "$staging" -czf "dist/$archive" "$name"

cd dist
if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$archive" > "$archive.sha256"
else
    shasum -a 256 "$archive" > "$archive.sha256"
fi
