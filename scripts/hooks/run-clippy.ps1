# Run cargo clippy on Rust file edits
# Provides immediate feedback on code quality

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

    if ($pathArg -match '\.rs$') {
        # Run clippy quietly, show only first 30 lines of output
        cargo clippy --quiet 2>&1 | Select-Object -First 30
    }
}
