# Prints one version's section of CHANGELOG.md (from "## 0.3.0" up to the
# next "## "), or fails when it is missing or empty: a version's notes are
# written in the pull request that bumps it.
param([Parameter(Mandatory)][string]$Version)

$path = Join-Path $PSScriptRoot '../../CHANGELOG.md'
$lines = @(Get-Content -Path $path -Encoding utf8)
$header = "^##\s+v?$([regex]::Escape($Version))(\s|$)"
$start = -1
for ($i = 0; $i -lt $lines.Count; $i++) {
  if ($lines[$i] -match $header) { $start = $i; break }
}
if ($start -lt 0) {
  Write-Error "O CHANGELOG.md não tem a seção da versão $Version (## $Version): escreva as notas dela no PR que sobe a versão."
  exit 1
}
$end = $start + 1
while ($end -lt $lines.Count -and $lines[$end] -notmatch '^##\s') { $end++ }
$body = if ($end -gt $start + 1) { ($lines[($start + 1)..($end - 1)] -join "`n").Trim() } else { '' }
if (-not $body) {
  Write-Error "A seção ## $Version do CHANGELOG.md está vazia."
  exit 1
}
$body
