# Branding Coding Folks — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The panel, app icon and marketing assets of Claude Switcher use the Coding Folks brand kit.

**Architecture:** Only token values in `ui/styles.css` change (the panel already styles everything through CSS variables), plus two bundled fonts, a redrawn SVG mark, and re-rendered PNG/ICO assets. No Rust or JS logic changes beyond one footer string.

**Tech Stack:** HTML/CSS (no build step), Playwright + axe (`tests-ui`), Edge for rendering, `cargo tauri icon` (tauri-cli 2.12).

**Spec:** `docs/design/branding-coding-folks.md`

## Global Constraints

- Token values exactly as the spec's table (light and dark), e.g. `--primary: #0D6AFF`, `--muted: #616874` / `#9CA1AA`, `--control: #898E99` / `#686E7C`, `--radius-card: 16px`, `--radius-control: 12px`.
- Fonts from `landing-page/public/fonts`, bundled in `ui/fonts/` with `OFL.txt`; never loaded from the network (CSP `default-src 'self'`).
- Orange (`--accent`) never as text colour or the only status signal.
- Mark: back `#073A8C`, front gradient `#094BB5`→`#3D88FF`, dot gradient `#DE7600`→`#F69B33`, card `rx` 181 (30% of 604).
- Every existing `tests-ui` test keeps passing (axe AA light/dark, forced-colors, menu/dialog, English).

## Review Focus

1. Instrument Sans is wider than Segoe UI → long account names and Indonesian strings still ellipsize or wrap, never overflow the 320 px panel.
2. Font file missing or blocked → panel falls back to Segoe UI and stays usable.
3. Dark mode: secondary buttons, the ⋯ menu and the add dialog keep ≥3:1 borders and ≥4.5:1 text.
4. Forced-colors (high contrast): brand colours do not leak in; system colours still win.
5. The new mark at 16 px (tray, taskbar) still reads as two cards and a dot.

---

### Task 1: Panel tokens, fonts, footer

**Files:**
- Create: `ui/fonts/instrument-sans-latin-wght-normal.woff2`, `ui/fonts/ibm-plex-mono-latin-500-normal.woff2` (or the 400/700 file present), `ui/fonts/OFL.txt`
- Modify: `ui/styles.css` (tokens, `@font-face`, `--font`, `--font-mono` on `.num`, weights 652/600/456), `ui/index.html` (footer line), `ui/strings.js` (`by_coding_folks` in en and id)
- Test: `tests-ui/panel.spec.ts`

- [ ] **Step 1: Tests** in `panel.spec.ts`: `brand fonts load from the app` (`document.fonts.check('16px "Instrument Sans"')` true after `document.fonts.ready`; the primary button's computed `font-family` starts with `"Instrument Sans"`); `primary button uses the brand blue` (computed background `rgb(13, 106, 255)` in light and dark); `footer says by Coding Folks` (en "by Coding Folks", id "oleh Coding Folks"); `a 40-character account name does not widen the panel` (Review Focus 1: `document.documentElement.scrollWidth <= 320` with mock data where one label is 40 × "W").
- [ ] **Step 2:** `cd tests-ui && npx playwright test` → the new tests fail.
- [ ] **Step 3:** Implement per the spec table.
- [ ] **Step 4:** Whole `tests-ui` suite green; look at light/dark screenshots next to the landing page.
- [ ] **Step 5:** Commit `feat(ui): panel follows the Coding Folks brand kit`.

### Task 2: Mark and app icons

**Files:**
- Modify: `assets/brand/logo-mark.svg`, `ui/index.html` (inline header mark), `ui/styles.css` (`.mark-*` fills), `tests-ui/scripts/render-brand.mjs` (fonts from `ui/fonts`, kit colours)
- Regenerate: `assets/brand/icon-1024.png`, `src-tauri/icons/*` via `cargo tauri icon assets/brand/icon-1024.png`

- [ ] **Step 1:** Redraw the SVG with the constraint values; same inline in the header.
- [ ] **Step 2:** `node tests-ui/scripts/render-brand.mjs` → `icon-1024.png`; `cd src-tauri && cargo tauri icon ../assets/brand/icon-1024.png`.
- [ ] **Step 3:** Look at 1024 px and 16/32 px (Review Focus 5); `cargo build` still succeeds; `tests-ui` green.
- [ ] **Step 4:** Commit `feat(brand): app icon in the Coding Folks colours`.

### Task 3: Social preview, README screenshots, docs

**Files:**
- Modify: `tests-ui/scripts/render-brand.mjs` (og-image layout), `CHANGELOG.md`, `docs/design/branding-coding-folks.md` (status)
- Regenerate: `assets/brand/social-preview.png`, `social-preview-dark.png`, `docs/assets/panel-*.png`

- [ ] **Step 1:** og-style lockup: blue gradient (`#0D6AFF`→`#3D88FF` direction as og-image), "Claude Switcher" white 652 with orange `.`, tagline from README, Plex Mono box `github.com/Coding-Folks/claude-switcher`, mark at right; dark variant on `#0D1117`.
- [ ] **Step 2:** Render; `node tests-ui/serve.mjs` + `node tests-ui/scripts/docs-shots.mjs`.
- [ ] **Step 3:** Look at every image; CHANGELOG (Changed) entry.
- [ ] **Step 4:** Commit `docs(brand): social preview and screenshots in the Coding Folks style`.
