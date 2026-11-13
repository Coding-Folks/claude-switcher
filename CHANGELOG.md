# Changelog

All notable changes are listed here. The format follows [Keep a Changelog](https://keepachangelog.com/)
and the project uses [Semantic Versioning](https://semver.org/).

## [Unreleased]

### Added

- English interface. The panel and tray menu follow the Windows display language (Indonesian →
  Bahasa Indonesia, anything else → English) until you pick one under **Language**; the Main account's
  default name follows the language too.
- Release workflow ready for Authenticode signing through the SignPath Foundation (app exe first, then
  the installer), plus a GitHub build provenance attestation and checksum for every installer; see
  `docs/CODE-SIGNING.md`.

## [0.1.0] — 2026-11-12

First public release.

### Added

- Tray app with a 320 px panel: every account, its status and RAM, and a RAM strip that says how many
  more accounts fit.
- **Same-Windows-user accounts** (default): prepared slot folders, no admin prompt, unlimited accounts.
- **Separate-Windows-user accounts**: hidden local users created through one UAC prompt, passwords in
  Windows Credential Manager, `~/.claude` shared through a junction.
- Import of the Code tab's session list into an account.
- Repair for refused sign-ins (including disabled or locked-out users), broken links and deleted folders.
- `Ctrl+Alt+K` and a tray menu to open the panel; autostart that follows the installed copy.
- Light, dark and high-contrast themes; WCAG 2.2 AA, tested with axe and Narrator.
- Coding Folks brand mark and app icons.

[Unreleased]: https://github.com/Coding-Folks/claude-switcher/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/Coding-Folks/claude-switcher/releases/tag/v0.1.0
