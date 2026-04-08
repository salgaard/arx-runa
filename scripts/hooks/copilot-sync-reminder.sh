#!/bin/bash
# Remind about copilot-sync when .claude/rules/ files are edited
# Keeps Claude and Copilot rules in sync

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
  
  if echo "$PATH_ARG" | grep -qE '/\.claude/rules/'; then
    echo "Reminder: .github/instructions/ counterpart may need updating — run /copilot-sync"
  fi
fi
