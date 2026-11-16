#!/bin/sh -e
# Build centaury and stage it as the Tauri sidecar (C1): tauri externalBin
# wants binaries/centaury-<target-triple>[.exe]. Run from anywhere.
# Target = TAURI_ENV_TARGET_TRIPLE (set by the tauri CLI for beforeBuildCommand,
# so `tauri build --target x86_64-apple-darwin` stages the matching binary),
# else the host triple.
cd "$(dirname "$0")/../.."
host=$(rustc -vV | sed -n 's/host: //p')
triple=${TAURI_ENV_TARGET_TRIPLE:-$host}
case "$triple" in *windows*) ext=.exe ;; *) ext= ;; esac
if [ "$triple" = "$host" ]; then
  cargo build -p centaury --release
  out=target/release
else
  cargo build -p centaury --release --target "$triple"
  out=target/$triple/release
fi
mkdir -p desktop/src-tauri/binaries
cp "$out/centaury$ext" "desktop/src-tauri/binaries/centaury-$triple$ext"
echo "sidecar staged: desktop/src-tauri/binaries/centaury-$triple$ext"
