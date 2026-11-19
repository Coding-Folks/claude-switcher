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
requested by the user or the person installing or operating it. It has no telemetry. Its only network
request is the update check described under [Updates](#updates), which runs only after the user
agrees to it in the panel (asked once, then a toggle; off until answered): it asks github.com for the
newest release's `latest.json` (no data about the user beyond what any HTTPS request reveals: the IP address
and the app's name and version), and installs an update only when the user presses **Update**
(**Perbarui**); see [SECURITY.md](../SECURITY.md).

## Updates

Installed copies can update themselves from GitHub releases (`tauri-plugin-updater`). Every update is
signed with a separate **update key**, independent of Authenticode, and the app refuses an installer
whose signature does not match the public key built into it.

**One-time setup (maintainer):**

1. On your own machine: `cargo tauri signer generate -w ~/.tauri/claude-switcher.key` and choose a
   password. Keep the private key file and password safe and backed up: if they are lost, installed
   copies can no longer be updated automatically.
2. Repository secrets: `TAURI_SIGNING_PRIVATE_KEY` = the content of `claude-switcher.key`,
   `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` = its password.
3. Put the content of `claude-switcher.key.pub` in `src-tauri/updater.pub` and commit it. The release
   workflow refuses to run when only one of the two is set up.

**Each release:** the workflow signs the final installer (after SignPath, when that is on) with the
update key, writes `latest.json` and attaches both to the draft release. Installed copies see it only
once the release is **published**, through
`https://github.com/Coding-Folks/claude-switcher/releases/latest/download/latest.json`. The tag must be
the app's version (`v` + `version` in `src-tauri/tauri.conf.json`); the workflow stops otherwise,
because a mismatch would offer the same release to installed copies again and again.

**Known limit:** the signature covers the installer, not the version number in `latest.json`
(`cargo tauri signer sign` does not record it), so someone able to change this repository's release
assets could offer an older, validly signed installer as "new". Protecting the repository and its
releases is therefore part of protecting updates.

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
