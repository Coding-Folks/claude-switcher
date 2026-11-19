# Writes the updater's latest.json for one Windows installer (format: v2.tauri.app/plugin/updater).
# The installer must already be signed for the updater: its signature is read from "<Setup>.sig".
# Usage: ./latest-json.ps1 -Setup dist/Claude-Switcher_0.3.0_x64-setup.exe -Tag v0.3.0 -Repo Coding-Folks/claude-switcher -Out dist/latest.json
param(
  [Parameter(Mandatory)] [string] $Setup,
  [Parameter(Mandatory)] [string] $Tag,
  [Parameter(Mandatory)] [string] $Repo,
  [Parameter(Mandatory)] [string] $Out
)
$ErrorActionPreference = 'Stop'

$version = $Tag.TrimStart('v')
if ($version -notmatch '^\d+\.\d+\.\d+([-+].+)?$') { throw "tag $Tag is not a version like v0.3.0" }
$sig = (Get-Content "$Setup.sig" -Raw).Trim()
if (-not $sig) { throw "$Setup.sig is empty" }
$name = Split-Path $Setup -Leaf

$latest = [ordered]@{
  version   = $version
  notes     = "https://github.com/$Repo/releases/tag/$Tag"
  pub_date  = (Get-Date).ToUniversalTime().ToString('yyyy-MM-ddTHH:mm:ssZ')
  platforms = [ordered]@{
    'windows-x86_64' = [ordered]@{
      signature = $sig
      url       = "https://github.com/$Repo/releases/download/$Tag/$name"
    }
  }
}
# UTF-8 without a BOM (the updater parses it as JSON).
[System.IO.File]::WriteAllText((Join-Path (Get-Location) $Out), ($latest | ConvertTo-Json -Depth 5))
