$env:Path = [System.Environment]::GetEnvironmentVariable("Path","Machine") + ";" + [System.Environment]::GetEnvironmentVariable("Path","User")

pandoc arx-runa-bachelorrapport.md `
  --pdf-engine=xelatex `
  --from=markdown+footnotes `
  --output=arx-runa-bachelorrapport.pdf `
  --lua-filter=table-wrap.lua `
  --lua-filter=path-break.lua `
  --include-before-body=cover.tex `
  --include-in-header=header-includes.tex `
  --variable=geometry:"a4paper,margin=2.5cm" `
  --variable=fontsize=11pt `
  --variable=lang=da `
  --toc

Write-Host "PDF genereret: arx-runa-bachelorrapport.pdf"
