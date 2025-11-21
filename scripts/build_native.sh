#!/bin/bash
set -e

echo "Building simple_git_cicd..."

# Build UI
echo "Building UI..."
cd ui
bun install
bun run build
cd ..

# Build Rust binary
echo "Building Rust binary..."
cargo build --release

echo ""
echo "Build complete!"
echo "Binary: ./target/release/simple_git_cicd"
echo "Size: $(ls -lh target/release/simple_git_cicd | awk '{print $5}')"
