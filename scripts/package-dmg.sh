#!/usr/bin/env bash
# Builds target/release/sayit-<version>.dmg: sayit.app plus an Applications
# shortcut for drag-to-install. Needs no admin rights; writes only inside target/.
set -euo pipefail
cd "$(dirname "$0")/.."

./scripts/bundle-macos.sh

VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)"
DMG="target/release/sayit-$VERSION.dmg"
STAGE="$(mktemp -d)"
trap 'rm -rf "$STAGE"' EXIT

cp -R target/release/sayit.app "$STAGE/"
ln -s /Applications "$STAGE/Applications"

hdiutil create -quiet -volname "sayit" -srcfolder "$STAGE" -format UDZO -ov "$DMG"
hdiutil verify -quiet "$DMG"
echo "Built $DMG ($(du -h "$DMG" | cut -f1))"
