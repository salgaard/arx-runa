# Remind about copilot-sync when .claude/rules/ files are edited
# Keeps Claude and Copilot rules in sync

$input = [Console]::In.ReadToEnd() | ConvertFrom-Json
$toolName = $input.toolName
$resultType = $input.toolResult.resultType

# Only run on successful edit operations
if ($resultType -ne "success") {
    exit 0
}

if ($toolName -in @("edit", "write", "create")) {
    $toolArgs = $input.toolArgs | ConvertFrom-Json
    $pathArg = if ($toolArgs.path) { $toolArgs.path } else { $toolArgs.file_path }

    if ($pathArg -match '[/\\]\.claude[/\\]rules[/\\]') {
        Write-Host "Reminder: .github/instructions/ counterpart may need updating - run /copilot-sync"
    }
}
