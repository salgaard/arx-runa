# Run cargo fmt on Rust file edits

$input = [Console]::In.ReadToEnd() | ConvertFrom-Json
$toolName = $input.toolName
$resultType = $input.toolResult.resultType

if ($resultType -ne "success") {
    exit 0
}

if ($toolName -in @("edit", "write", "create")) {
    $toolArgs = $input.toolArgs | ConvertFrom-Json
    $pathArg = if ($toolArgs.path) { $toolArgs.path } else { $toolArgs.file_path }

    if ($pathArg -match '\.rs$') {
        cargo fmt --quiet
    }
}
