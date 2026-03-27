#!/bin/bash
# Skill: Run a quick security lint
echo "Running Cargo Clippy with security focus..."
cargo clippy -- -D warnings
echo "Searching for non-zeroized sensitive strings..."
grep -r "String" src-tauri/src | grep -iE "pass|key|secret"