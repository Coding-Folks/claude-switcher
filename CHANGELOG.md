# Changelog

All notable changes are listed here. The format follows [Keep a Changelog](https://keepachangelog.com/)
and the project uses [Semantic Versioning](https://semver.org/).

## [Unreleased]

## [0.3.1] — 2026-11-20

### Fixed

- **Open Windows settings** (Google sign-in card and Settings) opened the Documents folder instead of
  Default apps. Links now open through the Windows shell; the GitHub links in Settings too.
- The panel now shrinks when a card goes away (such as **Later** on the sign-in card), instead of
  keeping its height with the content moved up.

## [0.3.0] — 2026-11-20

### Added

- **Usage per account, to pick one that is still safe.** Each Main and "This Windows user" row shows
  its Claude Code (and Cowork) token use in the last 5 hours, read from the transcripts on this
  computer; no login token is touched and nothing is sent anywhere. When Claude reports a usage limit
  the row says **Limit · back 19:00 (2 h 14 m)**; you can also mark it by hand (**⋯ → Mark as at its
  limit…**). After a limit, the panel learns that account's limit and shows **± N% of the limit**. A
  session continued in another account counts for the account that used it at the time.
- **Settings** (gear in the header): Start with Windows, update checks with **Check now** and
  **Update now**, language, the Google sign-in link setting (where **Later** leads), and **About &
  contributing** with links to report a problem or help.

### Changed

- The list view keeps only the accounts; the switches and language moved to Settings.

## [0.2.2] — 2026-11-20

### Added

- **Continue a session in another account.** When one account hits its usage limit, **⋯ → Continue a
  session in another account…** copies one of its recent Claude Code sessions to another account of
  this Windows user and opens that account with it; the conversation carries on with its full
  context. The session stays in the first account too, and a newer copy in the other account is never
  overwritten. Main and "This Windows user" accounts for now.

### Fixed

- After Claude Desktop updated itself, accounts' taskbar icons stopped being made until Claude
  Switcher restarted (it kept looking for Claude's logo in the old version's folder). It now finds
  Claude's new folder by itself, at most every ten minutes while the logo is missing.

## [0.2.1] — 2026-11-20

### Fixed

- Google sign-in in a new account still finished in Main with current Claude Desktop: Claude now
  claims `claude:` links in its package manifest, and Windows prefers that over the handler Claude
  Switcher set up. Claude Switcher now registers as an app that can open CLAUDE links, and while
  Windows still gives them to Claude the panel says so and opens **Settings → Default apps** for
  Claude Switcher: choose it for **CLAUDE** once and sign-ins land in the account that started them.

- A console window flashed up when Claude Switcher started (and when it closed or repaired an
  account): the PowerShell, icacls and taskkill calls now run without a window.
- The panel drifted up, away from the tray, when it got shorter (an account removed, the add form
  closed) and could slide under the taskbar when it grew: it now keeps the edge next to the taskbar
  in place.

## [0.2.0] — 2026-11-20

### Added

- English interface. The panel and tray menu follow the Windows display language (Indonesian →
  Bahasa Indonesia, anything else → English) until you pick one under **Language**; the Main account's
  default name follows the language too.
- Release workflow ready for Authenticode signing through the SignPath Foundation (app exe first, then
  the installer), plus a GitHub build provenance attestation and checksum for every installer; see
  `docs/CODE-SIGNING.md`.
- Every account other than Main gets its own taskbar button: a Claude icon in the account's colour
  (given automatically) and "Claude – <name>", in Alt+Tab too. Right-click → **Pin to taskbar** pins
  that account, and a "Claude – <name>" shortcut appears in the Start Menu under Claude Switcher.
  Main keeps Claude's own button, so existing pins are untouched.

- Updates from GitHub, only with your consent (asked once, then **Check for new versions** in the
  footer): the panel says when a newer version is out, and **Perbarui** downloads it,
  checks its signature, asks for UAC once and restarts into it. Nothing installs on its own. Needs the
  one-time update key setup in `docs/CODE-SIGNING.md#updates`; until then the check is off.
- Many accounts: the account list scrolls after about six rows instead of growing the panel, and a
  search field appears above eight accounts.

### Changed

- Panel controls follow the landing page components: pill buttons (36 and 44 px), 44 px inputs with
  the brand focus ring, a language picker with its own popover, and a pill toggle for **Start with
  Windows**. Spacing is on a 4 px grid, and the empty part of the RAM bar is visible again in light
  mode.

- The panel, app icon and social preview follow the Coding Folks brand kit: Instrument Sans with IBM
  Plex Mono for numbers (bundled, never downloaded), the brand blue on the main button, the kit's
  greys and rounded cards, and "by Coding Folks" in the footer. The app icon keeps its two cards and
  orange dot in the kit's exact colours with the kit's app-icon corners. Contrast stays WCAG 2.2 AA in
  light, dark and high-contrast.

### Fixed

- Signing in with Google in a new account (this Windows user) finished in the Main account instead:
  the browser hands the result back with a `claude://` link, which Windows always opened in Main.
  Claude Switcher now handles `claude://` links and passes each one to the account that most recently
  started a sign-in (within 15 minutes), and to Main otherwise.

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

[Unreleased]: https://github.com/Coding-Folks/claude-switcher/compare/v0.3.1...HEAD
[0.3.1]: https://github.com/Coding-Folks/claude-switcher/compare/v0.3.0...v0.3.1
[0.3.0]: https://github.com/Coding-Folks/claude-switcher/compare/v0.2.2...v0.3.0
[0.2.2]: https://github.com/Coding-Folks/claude-switcher/compare/v0.2.1...v0.2.2
[0.2.1]: https://github.com/Coding-Folks/claude-switcher/compare/v0.2.0...v0.2.1
[0.2.0]: https://github.com/Coding-Folks/claude-switcher/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/Coding-Folks/claude-switcher/releases/tag/v0.1.0
