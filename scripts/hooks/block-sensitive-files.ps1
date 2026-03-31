# Block access to sensitive files (.env, secrets/)
# Prevents accidental exposure of credentials

$input = [Console]::In.ReadToEnd() | ConvertFrom-Json
$toolName = $input.toolName

if ($toolName -in @("view", "read", "edit", "write", "create")) {
    $toolArgs = $input.toolArgs | ConvertFrom-Json
    $pathArg = if ($toolArgs.path) { $toolArgs.path } else { $toolArgs.file_path }

    if ($pathArg -match '(\.env$|\.env\.|[/\\]secrets[/\\])') {
        @{
            permissionDecision = "deny"
            permissionDecisionReason = "Blocked: sensitive file access rejected (.env or secrets/)"
        } | ConvertTo-Json -Compress
        exit 0
    }
}

# Allow by default (no output)
