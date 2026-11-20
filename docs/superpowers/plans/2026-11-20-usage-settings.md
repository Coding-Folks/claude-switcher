# Pemakaian per akun, Pengaturan, update sekarang, kontribusi — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Each account row shows its recent Claude Code usage and limit state so the user can pick an account that is still safe; a Settings view holds the preferences, "check / update now", the sign-in link setting and how to contribute.

**Architecture:** `usage.rs` reads the shared `~/.claude/projects` transcripts incrementally and attributes them to accounts through each account's session files (`sessions.rs`); a 60 s background loop keeps per-account sums and limit events in state; `AccountView` carries them to the panel. Learned limits and manual marks live in `accounts.json`. The panel gets a usage line per row and a second view, Settings.

**Tech Stack:** Rust (serde_json, chrono), Tauri 2, vanilla JS panel, Playwright + axe.

**Spec:** `docs/design/usage-settings-and-contributing.md`

## Global Constraints

- Windows: 5 h and 7 d, by message `timestamp`; tokens = input + cache_creation + output + 0.1 × cache_read (`CACHE_READ_WEIGHT = 0.1`, one constant).
- Never read or send login tokens; nothing leaves the machine.
- Limit: `isApiErrorMessage: true` and (`error` contains `rate_limit` or text contains "limit" and ("reset" or "resets")), within the last 5 h → `limited_until` from the text ("resets 7pm", "resets at 19:00", "|<epoch seconds>") or error time + 5 h.
- Learned limit: average of the last 3 `usage_5h` values at the moment an account becomes limited (auto or manual), stored as `limit_samples: Vec<u64>` (max 3) on `Account`.
- Manual mark: `limit_mark_until: Option<i64>` (ms) on `Account`; default +5 h; "clear" removes it.
- Only Main and same-user slots get usage; `windows_user` rows show no usage line.
- Settings view in the same window; Esc / back button returns; focus goes to the back button on open.
- `open_url` only opens a fixed allow-list (repo, issues/new?template=bug_report.yml, CONTRIBUTING.md, LICENSE).

## Review Focus

1. A huge transcript (hundreds of MB) or many sessions → the loop stays incremental and never blocks the panel.
2. A transcript line being written (partial JSON) → skipped now, counted on the next pass (offset not advanced past it).
3. Clock text in a limit message in 12 h or 24 h form, or none → a sane reset time; a reset time already past → treated as not limited.
4. The same `cliSessionId` in two accounts (a continued session) → its usage counts for the account whose session was active when each message was written? Not knowable → count for every account that lists it, and the help text says so.
5. Settings opened while an update downloads, or the add form open → back returns to the same list state; nothing lost.

---

### Task 1: `usage.rs` — reading transcripts

**Files:** Create `src-tauri/src/usage.rs`; Modify `src-tauri/src/lib.rs`

**Interfaces — Produces:**
- `pub struct Event { pub at_ms: i64, pub tokens: u64 }`, `pub struct LimitHit { pub at_ms: i64, pub until_ms: i64 }`
- `pub fn parse_line(line: &str) -> Option<Parsed>` where `enum Parsed { Usage(Event), Limit(LimitHit) }`
- `pub fn reset_time(text: &str, at_ms: i64, tz_offset_min: i32) -> i64`
- `pub struct Tracker` with `pub fn new(cache: PathBuf) -> Self`, `pub fn scan(&mut self, files: &[PathBuf])` (incremental, keeps per-file offset; partial last line not consumed), `pub fn totals(&self, files: &[PathBuf], now_ms: i64) -> Totals` → `Totals { five_h: u64, seven_d: u64, limited_until: Option<i64> }`, `save()`/load of the cache (offsets + events of the last 7 d per file)
- `pub fn transcript_files(claude_dir: &Path, cli_ids: &[String]) -> Vec<PathBuf>` (`projects/*/<id>.jsonl` and `projects/*/<id>/subagents/*.jsonl`)
- `pub fn learned(samples: &[u64]) -> Option<u64>` (mean, None if empty)

- [ ] **Step 1: Tests:** `usage_counts_input_cache_creation_output_and_a_tenth_of_cache_reads` (usage {2, 1000, 10000, 300} → 1302 + 1000 = 2302… exact: 2 + 1000 + 300 + 1000 = 2302); `lines_without_usage_or_timestamp_are_ignored`; `a_rate_limit_error_is_a_limit_hit`; `an_auth_error_is_not`; `reset_time_reads_12h_24h_and_epoch_forms` ("resets 7pm" at 15:00 local → 19:00; "resets at 19:00" → 19:00; "…|1795150800" → that epoch ×1000; none → at + 5 h; a time earlier than `at` → next day); `totals_sum_5h_and_7d_windows`; `a_limit_in_the_past_is_not_current`; `scan_is_incremental_and_waits_for_a_partial_last_line` (append half a line → not counted; complete it → counted once); `cache_survives_a_restart`; `transcript_files_include_subagents`; `learned_is_the_mean`.
- [ ] **Step 2:** `cargo test --lib usage` → fail. **Step 3:** implement. **Step 4:** pass + clippy. **Step 5:** commit `feat: read Claude Code usage and limit hits from transcripts`.

### Task 2: Usage in the flows and the account list

**Files:** Modify `src-tauri/src/sessions.rs` (`pub fn cli_ids(roots) -> Vec<String>`: `cliSessionId` + `priorCliSessionIds` of all session files, archived included), `src-tauri/src/store.rs` (`limit_samples`, `limit_mark_until`, serde default/skip), `src-tauri/src/flows.rs` (`AccountView` + fields, `mark_limited(id, until_ms: Option<i64>)`, `clear_limit(id)`, `record_usage(map)`), `src-tauri/src/lib.rs` (60 s loop), `src-tauri/src/commands.rs` (`mark_limited`, `clear_limit`)

**Interfaces — Produces:** `AccountView` gains `usage: Option<UsageView>`; `UsageView { five_h: u64, seven_d: u64, limited_until: Option<i64>, limit_source: Option<&'static str> /* "auto"|"manual" */, limit_tokens: Option<u64> }` (serde camel/snake like the rest: snake).

- [ ] **Step 1: Tests (Fake):** `usage_is_shown_for_main_and_slots_not_for_windows_users`; `a_manual_mark_limits_the_account_until_then_and_learns_the_limit` (record usage 5h=1000, mark → limited_until set, `limit_samples` == [1000]); `clear_limit_removes_the_mark`; `an_auto_limit_learns_once_per_hit` (two loop passes with the same hit → one sample); `samples_keep_the_last_three`; `cli_ids_include_prior_ids_and_archived_sessions` (sessions.rs).
- [ ] **Step 2:** fail. **Step 3:** implement; the loop: per eligible account → `cli_ids` → `transcript_files` → `Tracker::scan` → totals; store in a `Mutex<HashMap<id, Totals>>` in `Flows`; emit `status` when any value changes; `Tracker` cache at `%LOCALAPPDATA%\ClaudeSwitcher\usage-cache.json`. **Step 4:** `cargo test` green, clippy. **Step 5:** commit `feat: usage and limit state per account`.

### Task 3: Panel — usage line, limit, marks

**Files:** Modify `ui/app.js`, `ui/styles.css`, `ui/strings.js`, `ui/mock-invoke.js` (usage per mock account; `?limited=1` → Utama limited until +2 h 14 m), `tests-ui/panel.spec.ts`

- [ ] **Step 1: Tests:** `each Main and slot row shows its 5-hour usage` (Utama "5 jam … rb token"; Kantor row without usage line); `a limited account shows when it recovers` ("Kena limit · pulih …", row has `.is-limited`); `percent of the learned limit when known` ("± 30% dari batas"); `mark and clear a limit from the menu` (⋯ → "Tandai kena limit…" → dialog with time input default +5 h → row limited; ⋯ → "Hapus tanda limit" → back); `the usage line is explained to screen readers` (`aria-describedby` to a hidden help text); axe with `?limited=1`; 200 % zoom no overflow.
- [ ] **Step 2:** fail. **Step 3:** implement (token format: `rb`/`jt` and `k`/`M`; countdown refreshed every 30 s). **Step 4:** suite green; screenshots. **Step 5:** commit `feat(ui): usage and limits per account`.

### Task 4: Settings view, check/update now, sign-in link, contributing

**Files:** Modify `ui/index.html` (header gear, `#settings` view; move footer content into it), `ui/app.js`, `ui/styles.css`, `ui/strings.js`, `ui/mock-invoke.js`, `src-tauri/src/commands.rs` (`check_update_now`, `open_url(which: &str)` with allow-list), `src-tauri/src/update.rs` (`check_now(app) -> AppResult<Option<UpdateInfo>>`), `src-tauri/src/lib.rs`, `tests-ui/panel.spec.ts`, `README.md`, `README.id.md`, `CONTRIBUTING.md`, `CHANGELOG.md`, design status

- [ ] **Step 1: Tests:** Rust `open_url_only_opens_known_links` (`url_for("repo")`, `"bug"`, `"contribute"`, `"license"`; unknown → None). UI: `the gear opens Settings and back returns` (focus on back; Esc returns; list state kept — Review Focus 5); `settings holds autostart, update checks, language and the sign-in link`; `Cek sekarang reports up to date` / `…shows the new version and Perbarui sekarang` (mock `?update=1` after check); `the sign-in link section shows its state and opens Windows settings` (`?link=other`); `contributing links open through the app` (calls `open_url` with "bug" etc.); `the list view no longer has the footer controls`; axe on Settings light/dark; existing tests that used footer controls updated to open Settings first.
- [ ] **Step 2:** fail. **Step 3:** implement. **Step 4:** suites green; screenshots; README "How to help" section + CONTRIBUTING intro line; CHANGELOG Added. **Step 5:** commit `feat(ui): settings view, check and update now, how to contribute`.
