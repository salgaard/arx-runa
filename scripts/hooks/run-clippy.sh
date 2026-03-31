#!/bin/bash
# Run cargo clippy on Rust file edits
# Provides immediate feedback on code quality

INPUT=$(cat)
TOOL_NAME=$(echo "$INPUT" | jq -r '.toolName')
RESULT_TYPE=$(echo "$INPUT" | jq -r '.toolResult.resultType // empty')

# Only run on successful edit operations
if [ "$RESULT_TYPE" != "success" ]; then
  exit 0
fi

if [ "$TOOL_NAME" = "edit" ] || [ "$TOOL_NAME" = "write" ] || [ "$TOOL_NAME" = "create" ]; then
  TOOL_ARGS=$(echo "$INPUT" | jq -r '.toolArgs')
  PATH_ARG=$(echo "$TOOL_ARGS" | jq -r '.path // .file_path // empty')
  
  if echo "$PATH_ARG" | grep -q '\.rs$'; then
    # Run clippy quietly, show only first 30 lines of output
    cargo clippy --quiet 2>&1 | head -30
  fi
fi
