#!/bin/sh
# Build the browser version into _site/: the engine compiled to WebAssembly,
# the page, and delve's own source bundled as a sample that needs no network.
set -eu
cd "$(dirname "$0")/.."

cargo build --release -p delve-web --target wasm32-unknown-unknown

out=_site
rm -rf "$out"
mkdir -p "$out"
cp target/wasm32-unknown-unknown/release/delve_web.wasm "$out/delve.wasm"
cp web/index.html web/style.css web/app.js web/vercel.json "$out/"
[ -f web/og.png ] && cp web/og.png "$out/"
python3 web/gen_sample.py "$out/sample.js"
echo "built $out/ ($(du -sh "$out" | cut -f1))"
