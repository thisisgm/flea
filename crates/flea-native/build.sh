#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/../.." && pwd)"
DEST_DIR="$ROOT_DIR/ui/modules/FleaNative"

echo "==> 1. Building Rust core static library..."
cd "$SCRIPT_DIR"
cargo build --release

echo "==> 2. Building Qt 6 QML Extension Plugin with CMake..."
cd "$SCRIPT_DIR/cpp"
cmake -B build -S .
cmake --build build

echo "==> 3. Installing FleaNative QML module to $DEST_DIR..."
mkdir -p "$DEST_DIR"
cp -v build/FleaNative/* "$DEST_DIR/"
cp -v build/libfleanative.so "$DEST_DIR/"

echo "==> Build and installation of FleaNative completed successfully."
