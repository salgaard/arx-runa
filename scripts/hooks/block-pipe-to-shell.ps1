# Block pipe-to-shell patterns (curl|sh, wget|sh)
# Prevents remote code execution attacks

$input = [Console]::In.ReadToEnd() | ConvertFrom-Json
$toolName = $input.toolName

if ($toolName -eq "bash" -or $toolName -eq "shell") {
    $toolArgs = $input.toolArgs | ConvertFrom-Json
    $command = $toolArgs.command

    if ($command -match 'curl\s+.+\|\s*(ba)?sh|wget\s+.+\|\s*(ba)?sh') {
        @{
            permissionDecision = "deny"
            permissionDecisionReason = "Blocked: pipe-to-shell pattern rejected (curl|sh or wget|sh)"
        } | ConvertTo-Json -Compress
        exit 0
    }
}

# Allow by default (no output)
