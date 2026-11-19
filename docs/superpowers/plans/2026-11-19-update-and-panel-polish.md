# Update dari GitHub + panel polish — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Panel controls match the landing page components, the account list scrolls instead of growing, and the panel offers a signed update from GitHub that the user starts with **Perbarui**.

**Architecture:** CSS tokens/components in `ui/styles.css`; a vanilla-JS listbox replaces the language `<select>` in `ui/app.js`; the list gets a max height and an optional filter. A new `update.rs` wraps `tauri-plugin-updater` (registered only when `src-tauri/updater.pub` is non-empty), exposed as `get_update` / `install_update` commands and a 6-hour check loop. `release.yml` signs the final installer and writes `latest.json`.

**Tech Stack:** HTML/CSS/vanilla JS, Playwright + axe; Rust, Tauri 2, `tauri-plugin-updater` 2; GitHub Actions.

**Spec:** `docs/design/update-and-panel-polish.md`

## Global Constraints

- Buttons: `border-radius: 999px`; row/dialog buttons 36 px (`sm`), "Tambah akun" and form submit 44 px (`md`); outline border `--control`.
- Inputs 44 px, radius 16 px, padding-inline 16 px, focus = border `--primary` + `box-shadow: 0 0 0 3px var(--ring)`; `--ring` `#E7F0FF` light, `rgb(13 106 255 / .35)` dark.
- Toggle 40×24 pill, 20 px knob; off: `--surface` + `--control` border; on: `--primary`.
- `--track`: `#D1D3D8` light, `#2D3036` dark, used by the RAM strip and the "free" legend key.
- All spacing multiples of 4 px; edge 16, gap 12.
- List max ≈ 6 rows; search when accounts > 8.
- Update endpoint `https://github.com/Coding-Folks/claude-switcher/releases/latest/download/latest.json`; Windows `installMode: "passive"`; first check after 10 s, then every 6 h; plugin off when `updater.pub` is empty.
- Every existing `tests-ui` and `cargo test` test keeps passing.

## Review Focus

1. Listbox opened at the bottom of a short panel → popover stays inside the window (opens upward when there is no room below).
2. Offline / GitHub down / bad signature / UAC refused → a message in `#alert`, the row stays, Switcher keeps running.
3. Many accounts plus the add form open → panel height stays within the work area; the form stays reachable.
4. Forced-colors: toggle, listbox and pill buttons keep visible boundaries with system colours.
5. Search typed, then an account removed or renamed → filter re-applies; focus never lands on a hidden row.

---

### Task 1: Buttons, inputs, toggle, spacing, RAM track

**Files:** Modify `ui/styles.css`, `ui/index.html` (classes for `md` buttons if needed); Test `tests-ui/panel.spec.ts`

- [ ] **Step 1: Tests:** `buttons are pills of the landing page sizes` (every `.primary,.secondary` radius ≥ 18 px; `#add-open` height 44; a row button height 36); `text inputs are 44 px with a focus ring` (open add form, focus `#add-label`: height 44, box-shadow contains the ring colour); `autostart is a pill toggle` (`#autostart` 40×24, `appearance: none`; checked → background `rgb(13, 106, 255)`); `the RAM track differs from the panel ground` (light: `.strip` background ≠ body background); forced-colors axe still passes.
- [ ] **Step 2:** run → new tests fail.
- [ ] **Step 3:** Implement per Global Constraints; round odd spacings to 4 px multiples.
- [ ] **Step 4:** whole suite green; screenshots light/dark looked at.
- [ ] **Step 5:** Commit `feat(ui): pill buttons, inputs and toggle like the landing page`.

### Task 2: Language listbox

**Files:** Modify `ui/index.html` (replace `<select id="lang">` with trigger button + `ul[role=listbox]`), `ui/app.js` (listbox behaviour, `applyLang` wiring), `ui/styles.css`; Test `tests-ui/panel.spec.ts` (update existing language tests that use `#lang` select)

**Interfaces:** Produces `#lang` = `button[aria-haspopup=listbox]` labelled by the "Bahasa" label; `#lang-list` = `ul[role=listbox]` with `li[role=option][data-value]`.

- [ ] **Step 1: Tests:** `language picker opens with Enter and arrows, Esc returns focus`; `typing a letter jumps to that language`; `choosing with Enter switches the language and keeps focus on the picker` (adapt the existing test); `aria: trigger expanded state and selected option`; `axe AA with the language list open`; Review Focus 1: `the list stays inside the window` (bounding box bottom ≤ innerHeight).
- [ ] **Step 2:** run → fail.
- [ ] **Step 3:** Implement (popover markup/sizes per spec table; chevron/check SVG paths from landing `Select.tsx`; opens upward when space below < list height; click outside closes).
- [ ] **Step 4:** suite green.
- [ ] **Step 5:** Commit `feat(ui): language picker like the landing page select`.

### Task 3: Scrolling account list and search

**Files:** Modify `ui/styles.css`, `ui/app.js`, `ui/index.html`, `ui/strings.js`, `ui/mock-invoke.js` (`?many=12` → 12 accounts); Test `tests-ui/panel.spec.ts`

- [ ] **Step 1: Tests:** `twelve accounts: the list scrolls and the panel does not grow` (panel `scrollHeight` with 12 ≤ with 6 + 8 px; `.accounts` `scrollHeight > clientHeight`); `arrow keys reach the last account and scroll it into view`; `search appears above 8 accounts and filters by name, case-insensitive`; `Esc clears the search`; Review Focus 5: `rename while filtered re-applies the filter`; `no search with 5 accounts`.
- [ ] **Step 2:** run → fail.
- [ ] **Step 3:** Implement.
- [ ] **Step 4:** suite green.
- [ ] **Step 5:** Commit `feat(ui): account list scrolls, search above eight accounts`.

### Task 4: Update in the app

**Files:** Create `src-tauri/src/update.rs`, `src-tauri/updater.pub` (empty); Modify `src-tauri/Cargo.toml` (`tauri-plugin-updater = "2"`), `src-tauri/src/lib.rs` (conditional plugin, loop, commands), `src-tauri/src/commands.rs`, `src-tauri/tauri.conf.json` (`plugins.updater.windows.installMode`, `bundle.createUpdaterArtifacts: false` — signing happens in CI after SignPath), `src-tauri/capabilities/default.json` (none needed if only custom commands), `ui/index.html`, `ui/app.js`, `ui/strings.js`, `ui/mock-invoke.js` (`?update=1`, `?update=fail`), `ui/styles.css`; Test `tests-ui/panel.spec.ts`, Rust unit tests in `update.rs`

**Interfaces:** `update::pubkey() -> Option<&'static str>` (trimmed `include_str!`, `None` when empty); `UpdateInfo { version: String, notes_url: String }` (serde camel/snake as other commands); commands `get_update() -> Option<UpdateInfo>`, `install_update() -> AppResult<()>`; event `update-progress` `{ percent: u8 }`.

- [ ] **Step 1: Tests:** Rust: `an_empty_or_whitespace_key_turns_updates_off`, `notes_url_points_at_the_release_tag` (`notes_url("0.3.0") == "https://github.com/Coding-Folks/claude-switcher/releases/tag/v0.3.0"`). UI: `no update row without an update`; `update row shows the version and Perbarui` (`?update=1`); `Perbarui shows progress and disables the button`; Review Focus 2: `a failed update shows the error and keeps the row` (`?update=fail`); axe with the row.
- [ ] **Step 2:** run → fail.
- [ ] **Step 3:** Implement; loop: sleep 10 s, check, then every 6 h; errors logged, never panic.
- [ ] **Step 4:** `cargo test`, clippy, `tests-ui` green; `cargo build` with empty `updater.pub` starts without the plugin.
- [ ] **Step 5:** Commit `feat: tell about a new version in the panel, update with one click`.

### Task 5: Release signing, latest.json, docs

**Files:** Modify `.github/workflows/release.yml`, `README.md`, `README.id.md`, `docs/HOW-IT-WORKS.md`, `docs/CODE-SIGNING.md`, `CHANGELOG.md`, `docs/design/update-and-panel-polish.md` (status)

- [ ] **Step 1:** After the final (SignPath-signed or unsigned) installer exists: if `TAURI_SIGNING_PRIVATE_KEY` is set, `cargo tauri signer sign -k "$env:TAURI_SIGNING_PRIVATE_KEY" -p "$env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD" <setup.exe>`; PowerShell writes `latest.json` (`version` from the tag without `v`, `pub_date` RFC 3339 UTC, `notes` release URL, `platforms.windows-x86_64 {signature: <.sig content>, url: releases/download/<tag>/<setup file name>}`); upload both to the draft release.
- [ ] **Step 2:** Validate the workflow YAML (`actionlint` if available, else a YAML parse) and the generated JSON shape with a local dry run of the PowerShell block on a dummy file.
- [ ] **Step 3:** Docs: how to create the key once and which secrets/`updater.pub` to fill (CODE-SIGNING.md), "Updates" in READMEs, HOW-IT-WORKS network note (Switcher now contacts github.com every 6 h), CHANGELOG.
- [ ] **Step 4:** Commit `ci: sign updates and publish latest.json with each release`.
