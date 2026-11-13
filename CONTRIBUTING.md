# Contributing to Claude Switcher

Thanks for helping! Bug reports, translations, docs and code are all welcome. By taking part you agree
to our [Code of Conduct](CODE_OF_CONDUCT.md).

## Ways to help

- **Report a bug** with the [bug template](https://github.com/Coding-Folks/claude-switcher/issues/new/choose).
  Add the relevant lines of `%LOCALAPPDATA%\ClaudeSwitcher\logs\app.log`.
- **Translate the UI.** The panel's strings are in [`ui/strings.js`](ui/strings.js) (copy the `en`
  block) and the tray menu's in [`src-tauri/src/i18n.rs`](src-tauri/src/i18n.rs). Add the language to
  `LANGS` and to `Lang`; the UI tests check that every key exists in every language.
- **Pick an issue** labelled `good first issue` or `help wanted`, and say you are on it.
- **Security issues** go through [SECURITY.md](SECURITY.md), not public issues.

## Set up

Windows 10/11 x64 with:

- **Rust** stable, MSVC toolchain (`winget install Rustlang.Rustup`)
- **Visual Studio Build Tools** with *C++ build tools* and a *Windows 11 SDK*
- **Node.js 22** and **Microsoft Edge** (UI tests run in Edge, no browser download)
- **Tauri CLI**: `cargo install tauri-cli --version "^2" --locked`
- **Claude Desktop** installed, for the `live_` tests and for running the app

> **Git Bash pitfall:** Git ships its own `link.exe` in `/usr/bin`, which shadows the MSVC linker and
> breaks every build with "extra operand". Build from PowerShell or a *Developer PowerShell*, or drop
> `/usr/bin` from `PATH` for cargo.

## Run and test

```bash
cd src-tauri
cargo tauri dev                       # run the app
cargo test --lib                      # unit + integration tests (needs Claude Desktop for live_ tests)
cargo test --lib -- --skip live_      # what CI runs
cargo clippy --lib --tests -- -D warnings
cargo fmt --check
```

```bash
cd tests-ui
npm ci
npx playwright test                   # UI + accessibility (axe) tests against ui/ with mock data
```

You can also open the panel in a browser with fake data: `node tests-ui/serve.mjs`, then
`http://127.0.0.1:4317/index.html?mock=1`.

Some behaviour needs real Windows users and UAC and cannot run in CI. Before a release, walk through
[tests/manual/README.md](tests/manual/README.md).

## Project layout

| Path | What |
|---|---|
| `src-tauri/src/flows.rs` | Account flows (add, open, close, remove, import, repair) over the `Platform` trait |
| `src-tauri/src/platform.rs` | The trait and its Windows implementation |
| `src-tauri/src/slot.rs` | Same-Windows-user accounts (slot folders) |
| `src-tauri/src/winuser.rs`, `elevated.rs`, `vault.rs`, `share.rs`, `launcher.rs` | Separate-Windows-user accounts |
| `src-tauri/src/monitor.rs` | Which Claude instances run and their RAM |
| `ui/` | The tray panel (plain HTML/CSS/JS) |
| `tests-ui/` | Playwright + axe tests, mock data lives in `ui/mock-invoke.js` |
| `docs/design/` | Design specs and implementation plans (Bahasa Indonesia) |

## Pull requests

- Write the failing test first, then the fix; keep `cargo test`, `npx playwright test`, `clippy` and
  `fmt` green.
- Keep the UI accessible: native elements, visible focus, text for every status, 24 px targets, and
  run the axe tests.
- One topic per pull request, with a short description of what changes for users.
- Commit messages follow [Conventional Commits](https://www.conventionalcommits.org/)
  (`feat:`, `fix:`, `docs:`, `chore:` …).
- Anything that deletes files or touches Windows users needs a test that proves it cannot reach
  outside its own folder.
