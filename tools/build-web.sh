#!/usr/bin/env bash
# Builds the GitHub Pages site into web/: the WebAssembly renderer (web/pkg) and, when a MinGW cross compiler is
# installed, the Windows screensaver (web/Hajsaver.scr) for the download button.
set -euo pipefail
cd "$(dirname "$0")/.."
cargo build --release --lib --target wasm32-unknown-unknown
wasm-bindgen --target web --no-typescript --out-dir web/pkg target/wasm32-unknown-unknown/release/hajsaver.wasm
if command -v wasm-opt >/dev/null; then wasm-opt -O3 --all-features -o web/pkg/opt.wasm web/pkg/hajsaver_bg.wasm && mv web/pkg/opt.wasm web/pkg/hajsaver_bg.wasm || echo "wasm-opt failed, keeping the unoptimized build"; fi
if command -v x86_64-w64-mingw32-gcc >/dev/null; then
  cargo build --release --target x86_64-pc-windows-gnu
  cp target/x86_64-pc-windows-gnu/release/hajsaver.exe web/Hajsaver.scr
fi
ls -la web web/pkg
