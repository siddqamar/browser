#!/usr/bin/env bash
# Build the Rust engine (static lib + C header) then the Swift app that links it.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
MODE="${1:-debug}"   # debug | release

echo "==> Building Rust engine ($MODE)"
if [[ "$MODE" == "release" ]]; then
    cargo build --release --manifest-path "$ROOT/Cargo.toml"
    RUST_LIBDIR="$ROOT/target/release"
else
    cargo build --manifest-path "$ROOT/Cargo.toml"
    RUST_LIBDIR="$ROOT/target/debug"
fi

echo "==> Header at $ROOT/include/browser.h"
test -f "$ROOT/include/browser.h"
test -f "$RUST_LIBDIR/libbrowser_ffi.a"

echo "==> Building Swift app"
# Package.swift hardcodes `-L <ROOT>/target/debug -lbrowser_ffi`. Two consequences that silently
# ship a STALE engine:
#   1. The linker prefers the CDYLIB over the .a, so the app dynamically loads
#      target/debug/deps/libbrowser_ffi.dylib at runtime — and a release `cargo build` updates
#      target/release/..., NOT target/debug/..., so the app keeps loading the old dylib.
#   2. The linker searches target/debug FIRST, so even the static path resolves there.
# Fix: mirror the freshly-built lib (whatever MODE) into every target/debug location the app's
# Package.swift -L and the binary's baked dylib load path use.
for d in "$ROOT/target/debug" "$ROOT/target/debug/deps"; do
    mkdir -p "$d"
    for ext in a dylib; do
        src="$RUST_LIBDIR/libbrowser_ffi.$ext"; [[ -f "$src" ]] || src="$RUST_LIBDIR/deps/libbrowser_ffi.$ext"
        [[ -f "$src" ]] && cp -f "$src" "$d/libbrowser_ffi.$ext"
    done
done
# SwiftPM doesn't track the .a/.dylib as a dependency and caches the resolved lib, so an
# incremental build keeps linking a stale lib. A clean Swift build re-reads it. ~5s; correctness.
rm -rf "$ROOT/swift/.build"
if [[ "$MODE" == "release" ]]; then
    swift build --package-path "$ROOT/swift" -c release \
        -Xlinker -L"$RUST_LIBDIR"
    APP="$ROOT/swift/.build/release/Browser"
else
    swift build --package-path "$ROOT/swift"
    APP="$ROOT/swift/.build/debug/Browser"
fi

echo "==> Built: $APP"
echo "Run it with: $APP"
