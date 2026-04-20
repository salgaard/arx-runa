#!/bin/bash
# Block access to sensitive files (.env, secrets/)
# Prevents accidental exposure of credentials

INPUT=$(cat)
TOOL_NAME=$(echo "$INPUT" | jq -r '.toolName')

if [ "$TOOL_NAME" = "view" ] || [ "$TOOL_NAME" = "read" ] || [ "$TOOL_NAME" = "edit" ] || [ "$TOOL_NAME" = "write" ] || [ "$TOOL_NAME" = "create" ]; then
  TOOL_ARGS=$(echo "$INPUT" | jq -r '.toolArgs')
  PATH_ARG=$(echo "$TOOL_ARGS" | jq -r '.path // .file_path // empty')
  
  if echo "$PATH_ARG" | grep -qE '(\.env$|\.env\.|/secrets/)'; then
    echo '{"permissionDecision":"deny","permissionDecisionReason":"Blocked: sensitive file access rejected (.env or secrets/)"}'
    exit 0
  fi
fi

# Allow by default
