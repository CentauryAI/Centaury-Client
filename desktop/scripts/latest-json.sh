#!/bin/sh
# C5: emit a tauri-updater latest.json from built bundle artifacts.
# Usage: latest-json.sh <version> <url-base>
#   version  — version advertised to running apps (release job: tauri.conf version)
#   url-base — where the artifacts will be downloadable (no trailing slash)
# Reads sigs from the cargo bundle dir — workspace-root target/ first (our layout:
# desktop/src-tauri is a workspace member, so cargo writes to ../target), falling
# back to src-tauri/target for standalone checkouts. Linux updater = AppImage only
# (deb/rpm installs don't self-update); mac/windows arms appended when those
# bundles exist. Run from desktop/.
set -eu

VERSION="$1"
BASE="$2"
BUNDLE=../target/release/bundle
[ -d "$BUNDLE" ] || BUNDLE=src-tauri/target/release/bundle

appimage=$(ls "$BUNDLE"/appimage/*.AppImage 2>/dev/null | head -n1) || true
[ -n "${appimage:-}" ] || { echo "no AppImage in $BUNDLE/appimage" >&2; exit 1; }
sig="$appimage.sig"
[ -f "$sig" ] || { echo "missing $sig — build with createUpdaterArtifacts + signing key env" >&2; exit 1; }

cat <<EOF
{
  "version": "$VERSION",
  "pub_date": "$(date -u +%Y-%m-%dT%H:%M:%SZ)",
  "platforms": {
    "linux-x86_64": {
      "signature": "$(cat "$sig")",
      "url": "$BASE/$(basename "$appimage")"
    }
  }
}
EOF
