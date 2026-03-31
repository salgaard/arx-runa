#!/bin/bash
# Block pipe-to-shell patterns (curl|sh, wget|sh)
# Prevents remote code execution attacks

INPUT=$(cat)
TOOL_NAME=$(echo "$INPUT" | jq -r '.toolName')

if [ "$TOOL_NAME" = "bash" ] || [ "$TOOL_NAME" = "shell" ]; then
  COMMAND=$(echo "$INPUT" | jq -r '.toolArgs' | jq -r '.command // empty')
  
  if echo "$COMMAND" | grep -qE 'curl[[:space:]]+.+\|[[:space:]]*(ba)?sh|wget[[:space:]]+.+\|[[:space:]]*(ba)?sh'; then
    echo '{"permissionDecision":"deny","permissionDecisionReason":"Blocked: pipe-to-shell pattern rejected (curl|sh or wget|sh)"}'
    exit 0
  fi
fi

# Allow by default
