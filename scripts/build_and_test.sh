#!/bin/bash

cd ./src-tauri

echo "=== Running cargo build ==="
cargo build 2>&1
BUILD_EXIT_CODE=$?

echo ""
echo "=== Running cargo test --lib session:: ==="
cargo test --lib session:: 2>&1
TEST_EXIT_CODE=$?

echo ""
echo "=== Build Exit Code: $BUILD_EXIT_CODE ==="
echo "=== Test Exit Code: $TEST_EXIT_CODE ==="

if [ $BUILD_EXIT_CODE -eq 0 ] && [ $TEST_EXIT_CODE -eq 0 ]; then
    echo "✓ SUCCESS: Build and tests passed"
    exit 0
else
    echo "✗ FAILURE: Build or tests failed"
    exit 1
fi
