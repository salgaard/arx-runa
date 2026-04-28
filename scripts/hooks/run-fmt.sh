#!/bin/bash
# Run cargo fmt on Rust file edits

INPUT=$(cat)
TOOL_NAME=$(echo "$INPUT" | jq -r '.toolName')
RESULT_TYPE=$(echo "$INPUT" | jq -r '.toolResult.resultType // empty')

if [ "$RESULT_TYPE" != "success" ]; then
  exit 0
fi

if [ "$TOOL_NAME" = "edit" ] || [ "$TOOL_NAME" = "write" ] || [ "$TOOL_NAME" = "create" ]; then
  TOOL_ARGS=$(echo "$INPUT" | jq -r '.toolArgs')
  PATH_ARG=$(echo "$TOOL_ARGS" | jq -r '.path // .file_path // empty')

  if echo "$PATH_ARG" | grep -q '\.rs$'; then
    cargo fmt --quiet
  fi
fi
