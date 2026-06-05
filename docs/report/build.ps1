$env:Path = [System.Environment]::GetEnvironmentVariable("Path","Machine") + ";" + [System.Environment]::GetEnvironmentVariable("Path","User")

Push-Location $PSScriptRoot

$mdContent = [System.IO.File]::ReadAllText("$PSScriptRoot\arx-runa-bachelorrapport.md", [System.Text.Encoding]::UTF8)

# Pre-render mermaid diagrams til PNG
$mermaidDir = Join-Path $PSScriptRoot "mermaid-tmp"
New-Item -ItemType Directory -Force $mermaidDir | Out-Null

$mermaidPattern = [System.Text.RegularExpressions.Regex]::new('```mermaid\r?\n([\s\S]*?)\r?\n```')
$mermaidBlocks  = $mermaidPattern.Matches($mdContent)

Write-Host "Renderer $($mermaidBlocks.Count) mermaid-diagram(mer)..."

for ($i = $mermaidBlocks.Count - 1; $i -ge 0; $i--) {
    $m       = $mermaidBlocks[$i]
    $num     = $i + 1
    $mmdFile = Join-Path $mermaidDir "mermaid_$num.mmd"
    $pngFile = Join-Path $mermaidDir "mermaid_$num.png"

    [System.IO.File]::WriteAllText($mmdFile, $m.Groups[1].Value, [System.Text.Encoding]::UTF8)
    & "C:\Users\chris\AppData\Roaming\npm\mmdc.cmd" -i $mmdFile -o $pngFile -w 2000 -s 3 -b white 2>$null

    if (Test-Path $pngFile) {
        Write-Host "  OK: diagram $num"
        $pngRel = "mermaid-tmp/mermaid_$num.png"
        $replacement = "\noindent\makebox[\linewidth][c]{\includegraphics[width=205mm,height=0.9\textheight,keepaspectratio]{$pngRel}}"
    } else {
        Write-Warning "  FEJL: diagram $num - beholder kildekode"
        $replacement = $m.Value
    }
    $mdContent = $mdContent.Remove($m.Index, $m.Length).Insert($m.Index, $replacement)
}

# Pre-render kodeblokke til PNG via Pygments + Pillow
$renderScript = Join-Path $PSScriptRoot "render_code.py"
$codeDir = Join-Path $PSScriptRoot "code-tmp"
New-Item -ItemType Directory -Force $codeDir | Out-Null

# Match kodeblokke med angivet sprog (undtagen mermaid som allerede er håndteret)
$codePattern = [System.Text.RegularExpressions.Regex]::new('```(?!mermaid)(\w+)\r?\n([\s\S]*?)\r?\n```')
$codeBlocks  = $codePattern.Matches($mdContent)

Write-Host "Renderer $($codeBlocks.Count) kodeblok(ke) som billeder..."

for ($i = $codeBlocks.Count - 1; $i -ge 0; $i--) {
    $m        = $codeBlocks[$i]
    $num      = $i + 1
    $lang     = $m.Groups[1].Value
    $code     = $m.Groups[2].Value
    $srcFile  = Join-Path $codeDir "code_$num.$lang"
    $pngFile  = Join-Path $codeDir "code_$num.png"

    [System.IO.File]::WriteAllText($srcFile, $code, [System.Text.Encoding]::UTF8)

    python3 $renderScript $srcFile $lang $pngFile 2>$null

    if (Test-Path $pngFile) {
        Write-Host "  OK: kodeblok $num ($lang)"
        $pngRel = "code-tmp/code_$num.png"
        $replacement = "\noindent\makebox[\linewidth][c]{\includegraphics[width=\linewidth,keepaspectratio]{$pngRel}}"
    } else {
        Write-Warning "  FEJL: kodeblok $num ($lang) - beholder kildekode"
        $replacement = $m.Value
    }
    $mdContent = $mdContent.Remove($m.Index, $m.Length).Insert($m.Index, $replacement)
}

$tmpMd = Join-Path $PSScriptRoot "arx-runa-bachelorrapport-build.md"
[System.IO.File]::WriteAllText($tmpMd, $mdContent, [System.Text.Encoding]::UTF8)

pandoc $tmpMd `
  --pdf-engine=lualatex `
  --from=markdown+footnotes `
  --output=arx-runa-bachelorrapport.pdf `
  --lua-filter=table-wrap.lua `
  --lua-filter=table-grid.lua `
  --lua-filter=path-break.lua `
  --include-before-body=cover.tex `
  --include-in-header=header-includes.tex `
  --variable=geometry:"a4paper,margin=2.5cm" `
  --variable=fontsize=11pt `
  --variable=lang=da `
  --syntax-highlighting=tango `
  --toc

Remove-Item $tmpMd -Force -ErrorAction SilentlyContinue
Pop-Location

Write-Host "PDF genereret: arx-runa-bachelorrapport.pdf"
