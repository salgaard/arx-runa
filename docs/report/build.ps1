$env:Path = [System.Environment]::GetEnvironmentVariable("Path","Machine") + ";" + [System.Environment]::GetEnvironmentVariable("Path","User")

Push-Location $PSScriptRoot

# Pre-render mermaid diagrams til PNG
$imgDir = Join-Path $PSScriptRoot "mermaid-tmp"
New-Item -ItemType Directory -Force $imgDir | Out-Null

$mdContent = [System.IO.File]::ReadAllText("$PSScriptRoot\arx-runa-bachelorrapport.md", [System.Text.Encoding]::UTF8)
$pattern   = [System.Text.RegularExpressions.Regex]::new('```mermaid\r?\n([\s\S]*?)\r?\n```')
$blocks    = $pattern.Matches($mdContent)

Write-Host "Renderer $($blocks.Count) mermaid-diagram(mer)..."

for ($i = $blocks.Count - 1; $i -ge 0; $i--) {
    $m       = $blocks[$i]
    $num     = $i + 1
    $mmdFile = Join-Path $imgDir "mermaid_$num.mmd"
    $pngFile = Join-Path $imgDir "mermaid_$num.png"

    [System.IO.File]::WriteAllText($mmdFile, $m.Groups[1].Value, [System.Text.Encoding]::UTF8)
    & "C:\Users\chris\AppData\Roaming\npm\mmdc.cmd" -i $mmdFile -o $pngFile -w 2000 -b white 2>$null

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
