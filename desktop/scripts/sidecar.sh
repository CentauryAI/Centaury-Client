#!/bin/sh -e
# Build centaury and stage it as the Tauri sidecar (C1): tauri externalBin
# wants binaries/centaury-<host-triple>. Run from anywhere.
# ponytail: host triple only — cross-compile matrix when CI packaging lands.
cd "$(dirname "$0")/../.."
cargo build -p centaury --release
triple=$(rustc -vV | sed -n 's/host: //p')
mkdir -p desktop/src-tauri/binaries
cp target/release/centaury "desktop/src-tauri/binaries/centaury-$triple"
echo "sidecar staged: desktop/src-tauri/binaries/centaury-$triple"
