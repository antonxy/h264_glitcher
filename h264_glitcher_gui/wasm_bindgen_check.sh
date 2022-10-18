#!/usr/bin/env bash
set -eu
script_path=$( cd "$(dirname "${BASH_SOURCE[0]}")" ; pwd -P )
cd "$script_path"

./setup_web.sh

CRATE_NAME=${PWD##*/} # assume crate name is the same as the folder name

# This is required to enable the web_sys clipboard API which eframe web uses
# https://rustwasm.github.io/wasm-bindgen/api/web_sys/struct.Clipboard.html
# https://rustwasm.github.io/docs/wasm-bindgen/web-sys/unstable-apis.html
export RUSTFLAGS=--cfg=web_sys_unstable_apis

echo "Building rust…"
BUILD=debug # debug builds are faster

cargo build --lib --target wasm32-unknown-unknown

TARGET="target"

echo "Generating JS bindings for wasm…"

rm -f "${CRATE_NAME}_bg.wasm" # Remove old output (if any)

TARGET_NAME="${CRATE_NAME}.wasm"
wasm-bindgen "${TARGET}/wasm32-unknown-unknown/$BUILD/$TARGET_NAME" \
  --out-dir . --no-modules --no-typescript

# Remove output:
rm -f "${CRATE_NAME}_bg.wasm"
rm -f "${CRATE_NAME}.js"
