# Changelog

All notable changes are listed here. The format follows [Keep a Changelog](https://keepachangelog.com/)
and the project uses [Semantic Versioning](https://semver.org/).

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

[0.1.0]: https://github.com/Coding-Folks/claude-switcher/releases/tag/v0.1.0
