# Taskbar per akun — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Every non-Main account's Claude windows get their own taskbar button with a recoloured Claude icon and "Claude – <label>", pinnable to reopen that account.

**Architecture:** One reconcile loop (every 2 s) in a new `taskbar` module: give accounts a colour, make sure each has an `.ico` in `%PUBLIC%` and a Start Menu `.lnk` carrying its AUMID, remove stale ones, and stamp the AUMID + relaunch properties + icon on the top-level windows of each running account. Rename/remove need no hooks: the next tick fixes shortcuts and icons. `claude-switcher.exe --launch <id>` opens an account (pins and shortcuts use it).

**Tech Stack:** Rust, `windows` 0.61 (Shell, PropertiesSystem, Com), `ico` 0.6 (already in Cargo.lock via tauri).

**Spec:** `docs/design/taskbar-per-account.md`

## Global Constraints

- Main account: never stamped, no shortcut, no colour; its AUMID stays Claude's own.
- AUMID: `CodingFolks.ClaudeSwitcher.Account.<id>`.
- Icons in `%PUBLIC%\ClaudeSwitcher\icons\<id>-<hash8>.ico`, hash over colour + source-icon bytes; 16, 24, 32, 48, 64, 256 px.
- Shortcuts in `%APPDATA%\Microsoft\Windows\Start Menu\Programs\Claude Switcher\Claude – <label>.lnk`, target `claude-switcher.exe --launch <id>`.
- Relaunch properties set before `PKEY_AppUserModel_ID`; loop interval 2 s.
- Palette (in order): `#2F6FDE` blue, `#1E9E6A` green, `#8B5CF6` violet, `#D9468C` pink, `#0E9AA7` teal, `#C9A227` gold, `#6B7280` slate, `#B91C1C` red.
- A failure to stamp a window (other user's window, access denied) is logged once per window, never shown as an account problem.

## Review Focus

1. Labels with characters illegal in file names (`Kerja/Klien: A?`) → shortcut name sanitised, never fails the tick.
2. Two accounts with the same label → two distinct shortcuts (suffix ` (2)`), not one overwriting the other.
3. More accounts than palette colours → colours repeat in order, never panic.
4. Claude not installed / source PNG missing → no icons, no stamping, loop keeps running quietly.
5. Account removed while its window is still stamped → shortcut and icon removed; window left alone (it closes with Claude).

---

### Task 1: Account colour

**Files:**
- Modify: `src-tauri/src/store.rs` (Account gets `color`), `src-tauri/src/flows.rs` (literal `Account {..}` gain `color: None`; new methods)
- Create: `src-tauri/src/taskbar/mod.rs` (palette + `next_color`)

**Interfaces:**
- Produces: `Account.color: Option<String>` (`#[serde(default, skip_serializing_if = "Option::is_none")]`);
  `taskbar::PALETTE: [&str; 8]`; `taskbar::next_color(used: &[&str], n: usize) -> &'static str`;
  `Flows::ensure_colors(&self) -> AppResult<Vec<Account>>` (assigns + saves under `self.write`, returns the list);
  `Flows::running_roots(&self) -> Vec<(Account, u32)>` (non-Main accounts that run, with root pid, via `running_in`).

- [ ] **Step 1: Tests** in `taskbar/mod.rs`: `next_color(&[], 0) == "#2F6FDE"`; `next_color(&["#2F6FDE"], 1) == "#1E9E6A"`; all 8 used, n=8 → `"#2F6FDE"` (repeat by index `n % 8`). In `flows.rs` tests: `ensure_colors_gives_slots_distinct_colours_and_main_none` (two slots via `add_slot` helper → colours `#2F6FDE`, `#1E9E6A`, Main `None`, second call changes nothing); old JSON without `color` loads (store test).
- [ ] **Step 2:** `cargo test taskbar flows store` → fail (missing items).
- [ ] **Step 3:** Implement. `next_color`: first palette entry not in `used`, else `PALETTE[n % 8]`.
- [ ] **Step 4:** `cargo test` → all pass.
- [ ] **Step 5:** Commit `feat: accounts get a taskbar colour`.

### Task 2: Recoloured icon

**Files:**
- Create: `src-tauri/src/taskbar/icon.rs`
- Modify: `src-tauri/Cargo.toml` (`ico = "0.6"`)

**Interfaces:**
- Consumes: Claude install dir (`WinPlatform::install_dir()`), sources `assets\Square44x44Logo.targetsize-{16,24,32,48,64,256}_altform-unplated.png`.
- Produces: `icon::recolor(rgba: &mut [u8], color: [u8; 3])`; `icon::parse_hex(s: &str) -> Option<[u8; 3]>`;
  `icon::file_name(id: &str, color: &str, source: &[u8]) -> String` (`<id>-<8 hex of FNV-1a>.ico`);
  `icon::ensure(dir: &Path, install_dir: &Path, id: &str, color: &str) -> AppResult<PathBuf>` (writes only when missing).

- [ ] **Step 1: Tests:** an orange plate pixel `[0xD9,0x77,0x57,255]` recoloured to blue has hue within 10° of `#2F6FDE` and similar lightness; a white pixel `[250,245,240,255]` stays within 6 per channel; alpha unchanged; `parse_hex("#2f6fde") == Some([0x2F,0x6F,0xDE])`, `parse_hex("blue") == None`; `file_name` differs when colour or source differs and is stable for equal input.
- [ ] **Step 2:** run → fail.
- [ ] **Step 3:** Implement. Recolour: pixels with HSL saturation > 0.25 take the target's hue and saturation, keep their own lightness shifted by (target L − 0.55); others untouched. `ensure`: read each PNG with `ico::IconImage::read_png`, recolour, `IconDir` of all sizes, write `tmp` then rename.
- [ ] **Step 4:** `cargo test icon` → pass; plus an `#[ignore]` test writing a real icon to `%TEMP%` for a visual check (look at it, then leave it ignored).
- [ ] **Step 5:** Commit `feat: recoloured Claude icon per account`.

### Task 3: Shortcuts and window stamping (Win32)

**Files:**
- Create: `src-tauri/src/taskbar/shell.rs`
- Modify: `src-tauri/Cargo.toml` (windows features `Win32_System_Com`, `Win32_System_Com_StructuredStorage`, `Win32_UI_Shell_PropertiesSystem`, `Win32_System_Variant` as needed)

**Interfaces:**
- Produces (pure): `shell::aumid(id) -> String`; `shell::relaunch_command(exe: &Path, id) -> String` (`"<exe>" --launch <id>`); `shell::display_name(label) -> String` (`Claude – <label>`); `shell::shortcut_names(labels: &[(&str /*id*/, &str /*label*/)]) -> Vec<(String /*id*/, String /*file name*/)>` (sanitise `<>:"/\|?*` and control chars to `_`, trim dots/spaces, duplicates get ` (2)`, ` (3)`).
- Produces (Win32): `shell::write_shortcut(path: &Path, exe: &Path, id: &str, icon: &Path, aumid: &str) -> AppResult<()>`; `shell::top_windows_of(pid: u32) -> Vec<isize>` (visible, unowned top-level HWNDs); `shell::window_aumid(hwnd) -> Option<String>`; `shell::stamp(hwnd, aumid, relaunch, name, icon: &Path) -> AppResult<()>` (relaunch props, then ID, then `WM_SETICON` small + big from `LoadImageW`).

- [ ] **Step 1: Tests** for the pure functions, including Review Focus 1–2: `shortcut_names(&[("a","Kerja/Klien: A?")])` → `"Claude – Kerja_Klien_ A_.lnk"`; two `"Kerja"` → `"Claude – Kerja.lnk"`, `"Claude – Kerja (2).lnk"`; `relaunch_command(Path::new(r"C:\P F\cs.exe"), "x") == r#""C:\P F\cs.exe" --launch x"#`.
- [ ] **Step 2:** run → fail.
- [ ] **Step 3:** Implement. COM initialised per call site (`CoInitializeEx` apartment-threaded, ignore `S_FALSE`/`RPC_E_CHANGED_MODE`). Shortcut: `IShellLinkW` + `IPropertyStore` (AUMID) + `IPersistFile::Save`. Window: `SHGetPropertyStoreForWindow`, `PROPVARIANT` from `&str`, `Commit`.
- [ ] **Step 4:** `cargo test shell` → pass; `cargo clippy --all-targets -- -D warnings` clean.
- [ ] **Step 5:** Commit `feat: start menu shortcut and window stamping per account`.

### Task 4: Reconcile loop, `--launch`, docs

**Files:**
- Modify: `src-tauri/src/taskbar/mod.rs` (`plan` + `tick`), `src-tauri/src/lib.rs` (loop thread, `--launch` in single-instance callback and at startup), `CHANGELOG.md`, `docs/HOW-IT-WORKS.md`, `docs/design/taskbar-per-account.md` (status → implemented)

**Interfaces:**
- Consumes: Tasks 1–3.
- Produces: `taskbar::launch_arg(args: &[String]) -> Option<String>`; `taskbar::stale(existing: &[String], wanted: &[String]) -> Vec<String>` (file names to delete in a folder, only `.lnk`/`.ico` we own); `taskbar::tick(flows: &Flows<WinPlatform>, exe: &Path, stamped: &mut HashSet<(isize, String)>)`.

- [ ] **Step 1: Tests:** `launch_arg(["x","--launch","id1"]) == Some("id1")`, missing value → `None`; `stale(["Claude – A.lnk","Claude – Old.lnk","notes.txt"], ["Claude – A.lnk"]) == ["Claude – Old.lnk"]` (Review Focus 5: removed account's files go, foreign files stay).
- [ ] **Step 2:** run → fail.
- [ ] **Step 3:** Implement `tick`: `ensure_colors` → icons (`ensure`) → shortcuts (write when missing or name changed) → delete `stale` in both folders → for `running_roots`, each `top_windows_of(pid)` whose `window_aumid` ≠ account AUMID → `stamp`; errors logged once per `(hwnd, id)`. Review Focus 4: when the install dir or sources are missing, skip icons/stamping and return. In `lib.rs`: spawn the loop (2 s) beside the deeplink loop; single-instance callback: `launch_arg` → `flows.open(id, false)` on a blocking thread then `commands::refresh`, else `show_panel`; same at startup when started with `--launch`.
- [ ] **Step 4:** `cargo test`, clippy, fmt → clean. Manual on Windows 11: open a slot → within ~2 s it leaves the Claude stack with a blue icon and "Claude – <label>"; right-click → Pin; close Claude; click the pin → that account opens; rename → shortcut renamed; remove → shortcut and icon gone.
- [ ] **Step 5:** Update CHANGELOG (Added), HOW-IT-WORKS (section + files table rows for `%PUBLIC%\ClaudeSwitcher\icons` and the Start Menu folder). Commit `feat: own taskbar button per account`.
