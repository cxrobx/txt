#!/usr/bin/env bash
# Builds txt-mcp and stages it where Tauri's `externalBin` looks for it
# (`<path>-<target triple>`); the bundle copies it to Contents/MacOS/txt-mcp.
# txt-mcp is in the app's own crate, and tauri-build refuses to compile that
# crate until the staged file exists, so an empty placeholder goes first.
set -euo pipefail
cd "$(dirname "$0")/../src-tauri"
triple="$(rustc -vV | awk '/^host:/{print $2}')"
mkdir -p binaries
[ -f "binaries/txt-mcp-$triple" ] || touch "binaries/txt-mcp-$triple"
cargo build --release --bin txt-mcp
cp target/release/txt-mcp "binaries/txt-mcp-$triple"
