#!/bin/bash
# PostToolUse hook: run rustfmt on any .rs file after Edit/Write

input=$(cat)
file_path=$(echo "$input" | jq -r '.tool_input.file_path // empty')

if [[ "$file_path" == *.rs && -f "$file_path" ]]; then
    rustfmt "$file_path" 2>/dev/null
fi

exit 0
