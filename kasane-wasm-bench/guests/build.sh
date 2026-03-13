#!/bin/bash
set -euo pipefail
cd "$(dirname "$0")"

mkdir -p ../fixtures

for guest in string-echo element-builder state-reader; do
    echo "Building $guest..."
    cargo build --manifest-path "$guest/Cargo.toml" \
        --target wasm32-unknown-unknown --release 2>&1
    wasm_name="${guest//-/_}"
    cp "$guest/target/wasm32-unknown-unknown/release/${wasm_name}.wasm" \
       "../fixtures/${guest}.wasm"
    size=$(stat -c%s "../fixtures/${guest}.wasm" 2>/dev/null || stat -f%z "../fixtures/${guest}.wasm" 2>/dev/null)
    echo "  → fixtures/${guest}.wasm (${size} bytes)"
done

echo "All guests built."
