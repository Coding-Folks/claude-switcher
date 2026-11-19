# Fails unless the release tag is the app's version (v + "version" in src-tauri/tauri.conf.json).
# A mismatch would make every installed copy see the release as new forever: an update loop.
param([Parameter(Mandatory)] [string] $Tag, [string] $Conf = 'src-tauri/tauri.conf.json')
$ErrorActionPreference = 'Stop'
$app = (Get-Content $Conf -Raw | ConvertFrom-Json).version
$tagged = $Tag.TrimStart('v')
if ($tagged -ne $app) {
  throw "Tag $Tag is not the app version $app ($Conf). Set ""version"" to $tagged there (and in src-tauri/Cargo.toml), commit, and tag again."
}
"tag $Tag = app version $app"
