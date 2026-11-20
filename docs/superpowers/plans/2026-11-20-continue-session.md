# Lanjutkan sesi di akun lain — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** From an account's ⋯ menu, copy one of its recent Claude Code sessions to another account of this Windows user, so the session continues there after a usage limit.

**Architecture:** A new `sessions.rs` lists session files (`local_*.json`) under an account's `claude-code-sessions` folders, finds the destination's active `<account>\<org>` folder, and writes an updated copy. `Flows::continue_in` closes the destination if it runs, copies, and opens it. The panel gets a menu item and a dialog.

**Tech Stack:** Rust (serde_json), Tauri 2 commands, vanilla JS panel, Playwright + axe.

**Spec:** `docs/design/continue-session-in-another-account.md`

## Global Constraints

- Accounts that take part: Main and same-user slots only (not `windows_user`).
- Source list: `isArchived` ≠ true, sorted by `lastActivityAt` descending, at most 8; unreadable JSON skipped; title fallback "Sesi tanpa judul" / "Untitled session".
- Destination folder: the most recently modified `claude-code-sessions\<account>\<org>` directory; none → `NotFound` with `err.continue_no_signin`.
- Copy: same file name; `lastFocusedAt` = now (ms, as a string of digits like the source); `isArchived` = false; temp file + rename; never overwrite a destination copy whose `lastActivityAt` is newer (`err.continue_newer`).
- Destination running → close and wait, write, then open it. Source untouched.

## Review Focus

1. Timestamps stored as numbers or as digit strings → both sort correctly.
2. Source and destination are the same Claude login (two slots signed in to one account) → copying still works (it's a no-op overwrite of an older-or-equal file).
3. A session file is being written by Claude while we read it (partial JSON) → skipped, never crashes the list.
4. Destination closes slowly (still running after 10 s) → `err.still_running` shown, nothing written.
5. Only one eligible account, or the source has no sessions → the menu item is absent.

---

### Task 1: `sessions.rs`

**Files:** Create `src-tauri/src/sessions.rs`; Modify `src-tauri/src/lib.rs` (`pub mod sessions;`)

**Interfaces — Produces:**
- `pub struct SessionSummary { pub id: String /* "local_…" */, pub title: String, pub folder: String /* last component of cwd */, pub last_activity: i64 /* ms */, pub path: PathBuf }` (Serialize, skip `path`)
- `pub fn list(dirs: &[PathBuf], max: usize) -> Vec<SessionSummary>` (dirs = `claude-code-sessions` roots)
- `pub fn target_dir(dirs: &[PathBuf]) -> Option<PathBuf>`
- `pub enum CopyPlan { Write(String /* json */), DestinationNewer }`
- `pub fn plan_copy(src_json: &str, dest_existing: Option<&str>, now_ms: i64) -> AppResult<CopyPlan>`
- `pub fn copy(src: &Path, dest_dir: &Path, now_ms: i64) -> AppResult<()>` (`DestinationNewer` → `AppError(Invalid, "err.continue_newer")`)

- [ ] **Step 1: Tests:** `lists_recent_sessions_newest_first_without_archived_or_broken_files` (three files, one archived, one invalid JSON → two, newest first, `folder` == "Coding Folks" for `cwd` `E:\Folder Kerjaan\Coding Folks`, untitled → "" title kept for the UI to fill); `timestamps_as_numbers_or_strings_sort_the_same` (Review Focus 1); `the_most_recently_changed_account_folder_is_the_target` (two `<acc>\<org>` dirs, set mtimes; `None` when no org dir); `a_copy_is_focused_now_and_unarchived` (`plan_copy` output has `lastFocusedAt` "now", `isArchived` false, other fields equal); `a_newer_copy_in_the_destination_is_kept` (`DestinationNewer`); `an_older_or_equal_copy_is_replaced`; `copy_writes_the_file_under_its_own_name` (temp dirs).
- [ ] **Step 2:** `cargo test --lib sessions` → fail.
- [ ] **Step 3:** Implement with `serde_json::Value` (keep unknown fields); timestamps parsed from number or digit string; mtime via `std::fs::metadata().modified()`.
- [ ] **Step 4:** tests pass; clippy clean.
- [ ] **Step 5:** Commit `feat: read and copy Claude Code session files between accounts`.

### Task 2: Flow and commands

**Files:** Modify `src-tauri/src/platform.rs` (trait `fn session_dirs(&self, data_root: Option<&Path>) -> Vec<PathBuf>`; Win: `history::sessions_dirs(data_root or real_local)`), `src-tauri/src/flows.rs` (Fake impl + methods + tests), `src-tauri/src/commands.rs`, `src-tauri/src/lib.rs`

**Interfaces — Consumes:** Task 1. **Produces:** `Flows::sessions_of(id) -> AppResult<Vec<SessionSummary>>` (8); `Flows::continue_in(session_id, from, to) -> AppResult<String /* title */>`; commands `list_sessions(id)`, `continue_session(session_id, from, to)`; `AccountView` unchanged.

- [ ] **Step 1: Tests (Fake platform with temp dirs):** `continue_in_copies_the_session_and_opens_the_destination` (launch recorded for `to`); `continue_in_closes_a_running_destination_first` (close recorded before the file appears); `windows_user_accounts_cannot_take_part` (`Invalid`); `no_signed_in_destination_is_not_found` (`err.continue_no_signin`); `unknown_session_is_not_found`; Review Focus 4: `a_destination_that_will_not_close_is_left_alone` (`err.still_running`, no file).
- [ ] **Step 2:** run → fail.
- [ ] **Step 3:** Implement (reuse `close_and_wait`, `launch` path of `open`); applog `continue session` with ids only.
- [ ] **Step 4:** `cargo test` green; clippy clean.
- [ ] **Step 5:** Commit `feat: continue a session in another account`.

### Task 3: Panel

**Files:** Modify `ui/app.js`, `ui/index.html` (dialog `#continue`), `ui/styles.css`, `ui/strings.js` (id + en), `ui/mock-invoke.js` (`list_sessions`, `continue_session`; `?sessions=0`), `tests-ui/panel.spec.ts`, `CHANGELOG.md`, `README.md`, `README.id.md`, `docs/design/…` (status)

- [ ] **Step 1: Tests:** `the menu offers to continue a session when another account can take it` (item "Lanjutkan sesi di akun lain…" on Utama; absent on a `windows_user` row); Review Focus 5: `no continue item without sessions` (`?sessions=0`); `the dialog preselects the newest session and another account` (first radio checked, picker shows "Slot B"); `continuing announces it and opens the destination` (`#live` "Sesi “Sesi terakhir wedding app” dilanjutkan di Slot B.", Slot B state Running/Opening); `a newer copy in the destination is reported` (`?continue=newer` → `#alert` text from `err.continue_newer`); `axe AA with the continue dialog open`; keyboard: arrows move between sessions, Esc closes and returns focus to the kebab.
- [ ] **Step 2:** run → fail.
- [ ] **Step 3:** Implement: dialog reuses `<dialog>` styling, radios in a scrolling list, the account picker as a second instance of the landing-page listbox (factor the picker code into a small `makePicker(button, list, options, onChoose)`), relative time strings (`baru saja`, `{n} mnt lalu`, `{n} jam lalu`, `{n} hari lalu` / en).
- [ ] **Step 4:** whole `tests-ui` suite green; look at light/dark screenshots of the dialog.
- [ ] **Step 5:** CHANGELOG (Added), README "Use" table row, commit `feat(ui): continue a session in another account`.
