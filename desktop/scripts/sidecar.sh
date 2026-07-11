#!/bin/sh -e
# Build kore-client and stage it as the Tauri sidecar (C1): tauri externalBin
# wants binaries/kore-client-<host-triple>. Run from anywhere.
# ponytail: host triple only — cross-compile matrix when CI packaging lands.
cd "$(dirname "$0")/../.."
cargo build -p kore-client --release
triple=$(rustc -vV | sed -n 's/host: //p')
mkdir -p desktop/src-tauri/binaries
cp target/release/kore-client "desktop/src-tauri/binaries/kore-client-$triple"
echo "sidecar staged: desktop/src-tauri/binaries/kore-client-$triple"
