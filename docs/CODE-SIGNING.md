# Code signing policy

Windows releases of Claude Switcher are built on GitHub Actions from this repository and signed there.
Nobody signs a build on their own computer.

> Free code signing provided by [SignPath.io](https://signpath.io), certificate by
> [SignPath Foundation](https://signpath.org).
>
> *Until our SignPath Foundation application is approved, installers are unsigned; see
> [Checking a download](#checking-a-download).*

## What is signed

| File | How |
|---|---|
| `claude-switcher.exe` (the app and its admin helper) | Authenticode, before it is packed into the installer |
| `Claude-Switcher_<version>_x64-setup.exe` | Authenticode |
| Every release asset | [GitHub build provenance attestation](https://docs.github.com/actions/security-for-github-actions/using-artifact-attestations) (Sigstore), tying the file to the workflow run and commit that built it |

Only tags pushed to this repository start [`release.yml`](../.github/workflows/release.yml). Each
signing request is approved by hand by an approver below after checking that the tag, commit and
workflow run belong together.

## Team roles

| Role | Who |
|---|---|
| Committers and reviewers | [Members of Coding Folks](https://github.com/orgs/Coding-Folks/people) with write access |
| Approvers | [@Bobogeng](https://github.com/Bobogeng) |

Everyone in these roles must have two-factor authentication on GitHub. Changes reach `main` only through
commits by committers; outside contributions come in as reviewed pull requests.

## Privacy

This program will not transfer any information to other networked systems unless specifically
requested by the user or the person installing or operating it. It has no telemetry, no update check
and no network requests of its own; see [SECURITY.md](../SECURITY.md).

## Checking a download

**Signature** (once signing is active). In PowerShell:

```powershell
Get-AuthenticodeSignature .\Claude-Switcher_0.2.0_x64-setup.exe | Format-List Status, SignerCertificate
```

`Status` must be `Valid`.

**Provenance** (every release from 0.2.0). With the [GitHub CLI](https://cli.github.com):

```bash
gh attestation verify Claude-Switcher_0.2.0_x64-setup.exe --repo Coding-Folks/claude-switcher
```

**Checksum** (every release). Compare `Get-FileHash .\<file>` with `SHA256SUMS.txt` on the release page.
