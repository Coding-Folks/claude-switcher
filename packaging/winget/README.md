# winget

Manifests for [winget](https://learn.microsoft.com/windows/package-manager/) (Windows Package
Manager), in the folder layout of [microsoft/winget-pkgs](https://github.com/microsoft/winget-pkgs).
Once accepted there, people can install with:

```powershell
winget install CodingFolks.ClaudeSwitcher
```

## Check before submitting

On Windows, from the repository root:

```powershell
winget validate --manifest packaging\winget\manifests\c\CodingFolks\ClaudeSwitcher\0.3.1
```

```powershell
winget settings --enable LocalManifestFiles
```

```powershell
winget install --manifest packaging\winget\manifests\c\CodingFolks\ClaudeSwitcher\0.3.1
```

The install asks for admin once (the installer is per-machine) and may show SmartScreen.

## Submit

Either copy the version folder into a fork of `microsoft/winget-pkgs` at the same path and open a
pull request, or let [wingetcreate](https://github.com/microsoft/winget-create) do it:

```powershell
wingetcreate submit packaging\winget\manifests\c\CodingFolks\ClaudeSwitcher\0.3.1
```

Microsoft's bots validate and scan the installer; a maintainer then merges it (often within days).

## A new version

After a release is published, add a folder for it (same three files: version, `InstallerUrl`,
`InstallerSha256` from `SHA256SUMS.txt`, `ReleaseDate`, `ReleaseNotesUrl`), or once the package is
in winget-pkgs:

```powershell
wingetcreate update CodingFolks.ClaudeSwitcher --version <version> --urls https://github.com/Coding-Folks/claude-switcher/releases/download/v<version>/Claude-Switcher_<version>_x64-setup.exe --submit
```
