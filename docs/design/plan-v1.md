# Claude Switcher Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Aplikasi tray Windows (Tauri v2) yang menjalankan banyak akun Claude Desktop bersamaan, satu akun per user Windows tersembunyi, dengan `~/.claude` dibagi lewat junction.

**Architecture:** Satu exe Rust berjalan sebagai user utama; operasi admin dijalankan oleh exe yang sama dalam mode `--elevated` lewat UAC. Logika alur (`flows`) bergantung pada trait `Platform` sehingga bisa diuji dengan fake; implementasi Windows nyata ada di modul terpisah. UI adalah HTML/CSS/JS polos di WebView2.

**Tech Stack:** Rust stable, Tauri 2, `windows` crate 0.58+, serde, rand, zeroize, sysinfo; UI tanpa framework; Playwright + @axe-core/playwright untuk tes UI.

**Spec:** `docs/design/design.md`

## Global Constraints

- Hanya Windows 11 x64; app utama berjalan **tanpa** admin.
- Password tidak pernah ditulis ke disk selain Credential Manager, dan tidak pernah ke log; di-zeroize setelah dipakai.
- Nama user Windows harus cocok `^claude-[a-z0-9-]{1,13}$` (maks 20 karakter total).
- Target kredensial: `ClaudeSwitcher/<windows_user>`, `CRED_TYPE_GENERIC`, `CRED_PERSIST_LOCAL_MACHINE`.
- Data app: `%APPDATA%\ClaudeSwitcher\accounts.json` (skema `version: 1`); log: `%LOCALAPPDATA%\ClaudeSwitcher\logs\app.log`, rotasi 5×1 MB.
- Operasi elevated hanya: `create_user`, `set_password`, `delete_user`.
- Ambang RAM: konfirmasi bila RAM bebas < 2 GB. Polling monitor: 5 detik. Batas tunggu buka: 30 detik. Batas tunggu tutup sebelum terminasi: 10 detik.
- Fallback package family: `Claude_pzs8sxrjxfjjc`; alias: `claude-desktop.exe`.
- Panel: lebar 320px, tinggi maks 480px. UI berbahasa Indonesia, string terpusat di `ui/strings.js`.
- Aksesibilitas: WCAG 2.2 AA (kontras teks ≥4,5:1, komponen ≥3:1, fokus ≥2px, target ≥24×24px, forced-colors, reduced-motion).

## Review Focus

1. **Label non-ASCII / kosong / emoji** ("Kantor 🚀", "  ", "Ünïcødé") → nama user tetap valid; fallback `claude-akun`. Tes di Task 2.
2. **Bentrok nama** dengan akun lain atau user Windows yang sudah ada → sufiks `-2`, `-3`, tetap ≤20 karakter. Tes di Task 2.
3. **Hapus akun tidak boleh menyentuh `~/.claude` utama** walau junction ada di profil yang dihapus → junction dilepas sebagai link sebelum folder profil dihapus. Tes di Task 5.
4. **Claude ter-update** (versi folder WindowsApps berubah) → path exe/family dibaca saat runtime, bukan hard-code. Tes di Task 6.
5. **App crash saat menulis `accounts.json`** → file lama utuh (tulis atomik); file rusak dibackup, bukan dibuang. Tes di Task 2.

---

## File Structure

```
src-tauri/
  Cargo.toml, tauri.conf.json, build.rs, capabilities/default.json, icons/
  src/
    main.rs        // pilih mode: tray atau --elevated
    error.rs       // AppError
    naming.rs      // label -> nama user Windows
    store.rs       // accounts.json
    vault.rs       // Credential Manager
    elevated.rs    // parsing & dispatch mode --elevated, pemanggil UAC
    winuser.rs     // NetUserAdd/Del, sembunyikan user
    share.rs       // ACE + junction + hapus profil aman
    claude_pkg.rs  // temukan package family & alias
    launcher.rs    // bootstrap, launch, focus, close
    monitor.rs     // status proses & RAM
    platform.rs    // trait Platform + WinPlatform
    flows.rs       // add/open/close/rename/remove/repair (pakai Platform)
    commands.rs    // perintah Tauri tipis di atas flows
    tray.rs        // ikon, menu klik kanan, posisi panel
ui/
  index.html, styles.css, app.js, strings.js, mock-invoke.js
tests-ui/
  package.json, playwright.config.ts, panel.spec.ts
tests/manual/README.md
```

---

### Task 1: Scaffold Tauri app + AppError

**Prasyarat (pengguna):** `rustup` terpasang (`winget install Rustlang.Rustup`), toolchain `stable-x86_64-pc-windows-msvc`; VS 2022 Build Tools sudah ada.

**Files:** Create `src-tauri/*` (Cargo.toml, tauri.conf.json, build.rs, capabilities/default.json, src/main.rs, src/error.rs), `ui/index.html` (placeholder).

**Interfaces — Produces:**
- `pub struct AppError { pub code: ErrorCode, pub message_id: &'static str, pub detail: Option<String> }` dengan `#[derive(Debug, Serialize)]`.
- `pub enum ErrorCode { UacCancelled, ClaudeMissing, LogonFailed, JunctionBroken, LowMemory, StoreCorrupt, NotFound, Io, Win32(u32) }`.
- `pub type AppResult<T> = Result<T, AppError>;`

- [ ] **Step 1:** Buat proyek dengan `cargo install tauri-cli --version "^2" --locked` lalu struktur di atas secara manual (frontendDist = `../ui`, tanpa bundler JS). Window `panel`: `visible: false`, `decorations: false`, `width: 320`, `resizable: false`, `skipTaskbar: true`, `alwaysOnTop: true`. Tambahkan `tauri-plugin-single-instance` (instance kedua → tampilkan panel) dan `tauri-plugin-autostart`.
- [ ] **Step 2:** Tes `error.rs`: `AppError` serialisasi ke `{"code":"UacCancelled","message_id":"err.uac_cancelled","detail":null}`.
- [ ] **Step 3:** `cargo test` → PASS; `cargo tauri dev` → app jalan tanpa jendela terlihat.
- [ ] **Step 4:** Commit `feat: scaffold tauri app and error type`.

### Task 2: naming + store

**Files:** Create `src-tauri/src/naming.rs`, `src-tauri/src/store.rs` (tes `#[cfg(test)]` di file yang sama).

**Interfaces — Produces:**
- `pub fn windows_user_for(label: &str, taken: &dyn Fn(&str) -> bool) -> String`
- `pub fn is_valid_windows_user(name: &str) -> bool`
- `#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)] pub enum AccountKind { Main, WindowsUser }` (serde `rename_all = "snake_case"`)
- `pub struct Account { pub id: String /* uuid v4 */, pub label: String, pub kind: AccountKind, pub windows_user: Option<String>, pub created_at: String /* RFC3339 */ }`
- `pub struct Store { path: PathBuf }` dengan `Store::new(path) -> Self`, `load(&self) -> AppResult<LoadOutcome>`, `save(&self, accounts: &[Account]) -> AppResult<()>`.
- `pub enum LoadOutcome { Ok(Vec<Account>), RecoveredFromCorrupt { accounts: Vec<Account>, backup: PathBuf } }` — saat file tidak ada atau rusak, daftar berisi satu akun `Main` berlabel "Utama".

- [ ] **Step 1: Tes gagal**
```rust
#[test] fn slug_basic() { assert_eq!(windows_user_for("Kantor", &|_| false), "claude-kantor"); }
#[test] fn slug_unicode_and_emoji() { assert_eq!(windows_user_for("Ünïcødé 🚀", &|_| false), "claude-unicode"); }
#[test] fn slug_empty_fallback() { assert_eq!(windows_user_for("   ", &|_| false), "claude-akun"); }
#[test] fn slug_truncates() { assert!(windows_user_for("sangat panjang sekali namanya", &|_| false).len() <= 20); }
#[test] fn slug_collision_suffix() {
    let taken = |n: &str| n == "claude-kantor" || n == "claude-kantor-2";
    assert_eq!(windows_user_for("Kantor", &taken), "claude-kantor-3");
}
#[test] fn every_generated_name_is_valid() { for l in ["a","Kantor","x".repeat(50).as_str(),"--"] { assert!(is_valid_windows_user(&windows_user_for(l, &|_| false))); } }
#[test] fn store_roundtrip() { /* save 2 akun ke tempdir, load -> sama */ }
#[test] fn store_missing_file_gives_main() { /* load -> Ok([Main "Utama"]) */ }
#[test] fn store_corrupt_is_backed_up() { /* tulis "{oops", load -> RecoveredFromCorrupt, backup ada & berisi "{oops" */ }
#[test] fn store_atomic_write_leaves_no_tmp() { /* setelah save tidak ada *.tmp */ }
```
- [ ] **Step 2:** `cargo test naming store` → FAIL (belum ada).
- [ ] **Step 3:** Implementasi. Slug: transliterasi diakritik via Unicode NFKD lalu buang non-`[a-z0-9]`, spasi → `-`, gabung `-` beruntun, potong agar total ≤20 (sisakan ruang sufiks), sufiks mulai `-2`. Simpan: tulis `accounts.json.tmp` lalu `std::fs::rename`. Backup: `accounts.json.bad-<YYYYMMDD-HHMMSS>`.
- [ ] **Step 4:** `cargo test naming store` → PASS.
- [ ] **Step 5:** Commit `feat: account naming and atomic store`.

### Task 3: vault

**Files:** Create `src-tauri/src/vault.rs`.

**Interfaces — Produces:** `pub fn put(user: &str, secret: &Zeroizing<String>) -> AppResult<()>`, `pub fn get(user: &str) -> AppResult<Zeroizing<String>>`, `pub fn delete(user: &str) -> AppResult<()>` (tidak error bila tidak ada), `pub fn generate_password() -> Zeroizing<String>` (32 karakter dari `OsRng`, campuran huruf besar/kecil/angka/simbol, minimal satu dari tiap kelas).

- [ ] **Step 1: Tes gagal** (berjalan di Windows nyata, target uji `ClaudeSwitcher/test-<uuid>`):
```rust
#[test] fn vault_roundtrip_and_delete() { put(u,&s)?; assert_eq!(*get(u)?, *s); delete(u)?; assert!(matches!(get(u), Err(e) if e.code==ErrorCode::NotFound)); delete(u)?; }
#[test] fn password_meets_policy() { let p = generate_password(); assert_eq!(p.chars().count(), 32); /* tiap kelas karakter ada */ }
```
- [ ] **Step 2:** `cargo test vault` → FAIL.
- [ ] **Step 3:** Implementasi dengan `CredWriteW/CredReadW/CredDeleteW/CredFree`; blob UTF-16.
- [ ] **Step 4:** `cargo test vault` → PASS.
- [ ] **Step 5:** Commit `feat: credential manager vault`.

### Task 4: elevated mode + winuser

**Files:** Create `src-tauri/src/elevated.rs`, `src-tauri/src/winuser.rs`; Modify `src-tauri/src/main.rs` (cabang `--elevated`).

**Interfaces — Produces:**
- `pub enum ElevatedOp { CreateUser { name: String }, SetPassword { name: String }, DeleteUser { name: String } }` (serde tagged `op`). Password **tidak** lewat argumen: dikirim via file sementara ber-ACL hanya untuk user utama, path-nya di argumen, dihapus oleh proses elevated setelah dibaca.
- `pub fn parse_args(args: &[String]) -> AppResult<(ElevatedOp, Option<PathBuf> /*secret file*/, PathBuf /*result file*/)>` — menolak nama yang tidak lolos `is_valid_windows_user`.
- `pub fn run_elevated(op: &ElevatedOp, secret: Option<&Path>) -> AppResult<()>` (dipanggil di proses admin).
- `pub fn request(op: ElevatedOp, secret: Option<&Zeroizing<String>>) -> AppResult<()>` (dipanggil app utama: `ShellExecuteExW` verb `runas`, tunggu proses, baca result file; `ERROR_CANCELLED` → `ErrorCode::UacCancelled`).
- winuser: `create(name, &Zeroizing<String>)`, `hide(name)`, `set_password(name, &Zeroizing<String>)`, `delete(name)` (juga hapus nilai registry `SpecialAccounts\UserList\<name>`), `exists(name) -> bool` (`NetUserGetInfo`, tanpa admin).

- [ ] **Step 1: Tes gagal**
```rust
#[test] fn rejects_bad_name() { assert!(parse_args(&args(r#"{"op":"delete_user","name":"Administrator"}"#)).is_err()); }
#[test] fn rejects_unknown_op() { assert!(parse_args(&args(r#"{"op":"run","cmd":"x"}"#)).is_err()); }
#[test] fn accepts_create() { assert!(matches!(parse_args(&args(r#"{"op":"create_user","name":"claude-kantor"}"#)).unwrap().0, ElevatedOp::CreateUser{..})); }
```
- [ ] **Step 2:** `cargo test elevated` → FAIL.
- [ ] **Step 3:** Implementasi. `create`: `NetUserAdd` level 1, `UF_SCRIPT|UF_DONT_EXPIRE_PASSWD|UF_PASSWD_CANT_CHANGE`, `NetLocalGroupAddMembers` ke SID Users (`S-1-5-32-545`, bukan nama grup yang terlokalisasi), lalu `hide`.
- [ ] **Step 4:** `cargo test elevated` → PASS. Uji manual `winuser` ditulis di Task 12.
- [ ] **Step 5:** Commit `feat: elevated operations and windows user management`.

### Task 5: share

**Files:** Create `src-tauri/src/share.rs`.

**Interfaces — Produces:**
- `pub fn grant_shared_access(main_claude_dir: &Path, user: &str) -> AppResult<()>` / `revoke_shared_access(...)` — ACE Modify, `OBJECT_INHERIT_ACE|CONTAINER_INHERIT_ACE`, idempoten.
- `pub fn junction_ps_snippet(main_claude_dir: &Path) -> String` — PowerShell yang membuat `$env:USERPROFILE\.claude` sebagai junction bila belum ada; bila ada folder biasa (bukan junction) → rename ke `.claude.bak-<waktu>` dulu.
- `pub fn is_junction_to(link: &Path, target: &Path) -> bool`.
- `pub fn remove_profile_safely(profile_dir: &Path) -> AppResult<()>` — cari semua reparse point di dalam profil, lepas sebagai link (`RemoveDirectoryW` pada junction), baru hapus pohon folder. Tidak pernah mengikuti reparse point.

- [ ] **Step 1: Tes gagal**
```rust
#[test] fn remove_profile_never_follows_junction() {
    // tmp/target/keep.txt ; tmp/profile/.claude -> junction ke tmp/target
    remove_profile_safely(&profile)?;
    assert!(!profile.exists()); assert!(target.join("keep.txt").exists());
}
#[test] fn junction_detection() { assert!(is_junction_to(&link, &target)); assert!(!is_junction_to(&plain_dir, &target)); }
#[test] fn grant_is_idempotent() { grant(..)?; grant(..)?; /* satu ACE untuk user tsb */ }
```
- [ ] **Step 2:** `cargo test share` → FAIL.
- [ ] **Step 3:** Implementasi (`GetNamedSecurityInfoW`/`SetEntriesInAclW`/`SetNamedSecurityInfoW`; deteksi junction via `FILE_ATTRIBUTE_REPARSE_POINT` + `FSCTL_GET_REPARSE_POINT`). Tes ACL memakai user lokal bawaan `S-1-5-32-545` sebagai trustee.
- [ ] **Step 4:** `cargo test share` → PASS.
- [ ] **Step 5:** Commit `feat: shared ~/.claude access and safe profile removal`.

### Task 6: claude_pkg + launcher

**Files:** Create `src-tauri/src/claude_pkg.rs`, `src-tauri/src/launcher.rs`.

**Interfaces — Produces:**
- `pub struct ClaudePkg { pub family: String, pub install_dir: PathBuf }`; `pub fn detect() -> AppResult<ClaudePkg>` (jalankan `powershell -NoProfile -Command "Get-AppxPackage Claude | ConvertTo-Json"`); `pub fn parse_appx_json(s: &str) -> AppResult<ClaudePkg>`; `ErrorCode::ClaudeMissing` bila kosong.
- `pub fn alias_for_profile(profile: &Path) -> PathBuf` = `<profile>\AppData\Local\Microsoft\WindowsApps\claude-desktop.exe`.
- `pub struct BootstrapReport { pub registered: bool, pub junction_ok: bool, pub alias_present: bool }`; `pub fn parse_bootstrap_log(s: &str) -> BootstrapReport`.
- `pub fn bootstrap(user: &str, pw: &Zeroizing<String>, pkg: &ClaudePkg, main_claude_dir: &Path) -> AppResult<BootstrapReport>` — `CreateProcessWithLogonW(LOGON_WITH_PROFILE)` menjalankan PowerShell: `Add-AppxPackage -RegisterByFamilyName -MainPackage <family>` + `junction_ps_snippet` + tulis log `key=value` ke `%TEMP%` user utama (file ber-ACL untuk user itu). Batas 120 detik.
- `pub fn launch_main() -> AppResult<()>`; `pub fn launch_as(user: &str, pw: &Zeroizing<String>) -> AppResult<u32 /*pid*/>`; `ERROR_LOGON_FAILURE` → `ErrorCode::LogonFailed`.
- `pub fn focus(pid: u32) -> AppResult<()>` (EnumWindows untuk pid → `ShowWindow(SW_RESTORE)` + `SetForegroundWindow`); `pub fn close(pid: u32) -> AppResult<()>` (WM_CLOSE, tunggu 10 detik, lalu `TerminateProcess` pada pohon).

- [ ] **Step 1: Tes gagal**
```rust
#[test] fn parses_appx_json_any_version() {
    let j = r#"{"Name":"Claude","PackageFamilyName":"Claude_pzs8sxrjxfjjc","InstallLocation":"C:\\Program Files\\WindowsApps\\Claude_9.9.9.0_x64__pzs8sxrjxfjjc"}"#;
    let p = parse_appx_json(j).unwrap(); assert_eq!(p.family, "Claude_pzs8sxrjxfjjc"); assert!(p.install_dir.ends_with("Claude_9.9.9.0_x64__pzs8sxrjxfjjc"));
}
#[test] fn missing_package_is_claude_missing() { assert_eq!(parse_appx_json("").unwrap_err().code, ErrorCode::ClaudeMissing); }
#[test] fn bootstrap_log_parsing() {
    let r = parse_bootstrap_log("register=ok\njunction=ok\nalias=ok\n"); assert!(r.registered && r.junction_ok && r.alias_present);
    let r = parse_bootstrap_log("register=fail:0x80073CF9\n"); assert!(!r.registered && !r.junction_ok);
}
```
- [ ] **Step 2:** `cargo test claude_pkg launcher` → FAIL.
- [ ] **Step 3:** Implementasi.
- [ ] **Step 4:** `cargo test claude_pkg launcher` → PASS.
- [ ] **Step 5:** Commit `feat: claude package detection and launcher`.

### Task 7: monitor

**Files:** Create `src-tauri/src/monitor.rs`.

**Interfaces — Produces:**
- `pub struct ProcInfo { pub pid: u32, pub parent: u32, pub exe: PathBuf, pub cmdline: String, pub owner: String, pub working_set: u64 }`
- `pub struct InstanceStatus { pub owner: String, pub root_pid: u32, pub bytes: u64 }`
- `pub fn instances(procs: &[ProcInfo], install_dir: &Path) -> Vec<InstanceStatus>` — akar = `Claude.exe` di bawah `install_dir`, `cmdline` tanpa `--type=`, parent bukan `Claude.exe`; `bytes` = jumlah working set seluruh keturunan.
- `pub fn snapshot() -> AppResult<(Vec<ProcInfo>, u64 /*free RAM bytes*/)>`.
- `pub fn spawn_poller(app: AppHandle, every: Duration)` — emit event `"status"` berisi `Vec<InstanceStatus>` + `free_ram` hanya bila berubah (bytes dibulatkan ke 100 MB untuk perbandingan).

- [ ] **Step 1: Tes gagal**
```rust
#[test] fn picks_roots_and_sums_tree() { /* 2 akar (main user, claude-kantor) + renderer --type= + claude-code CLI di Claude-Data (bukan install_dir) -> 2 instance, bytes = akar+anak */ }
#[test] fn ignores_cli_claude_exe() { /* claude.exe di Claude-Data\claude-code -> diabaikan */ }
```
- [ ] **Step 2:** `cargo test monitor` → FAIL.
- [ ] **Step 3:** Implementasi (`sysinfo` untuk daftar proses & memori; owner via `OpenProcessToken`+`LookupAccountSidW`).
- [ ] **Step 4:** `cargo test monitor` → PASS.
- [ ] **Step 5:** Commit `feat: process and memory monitor`.

### Task 8: platform + flows

**Files:** Create `src-tauri/src/platform.rs`, `src-tauri/src/flows.rs`.

**Interfaces:**
- Consumes: semua fungsi Task 2–7.
- Produces: `pub trait Platform` dengan metode: `user_exists(&str)->bool`, `vault_put/get/delete`, `elevated(ElevatedOp, Option<&Zeroizing<String>>)->AppResult<()>`, `grant/revoke(&str)->AppResult<()>`, `bootstrap(&str,&Zeroizing<String>)->AppResult<BootstrapReport>`, `launch_main()`, `launch_as(&str,&Zeroizing<String>)->AppResult<u32>`, `focus(u32)`, `close(u32)`, `running()->Vec<InstanceStatus>`, `free_ram()->u64`, `remove_profile(&str)->AppResult<()>`. `WinPlatform` mengimplementasikannya dengan modul nyata.
- `pub struct Flows<P: Platform> { p: P, store: Store }` dengan:
  `add(&self, label: &str) -> AppResult<Account>`,
  `open(&self, id: &str, confirmed_low_ram: bool) -> AppResult<OpenOutcome>` (`OpenOutcome::{Focused, Launched, NeedsLowRamConfirm}`),
  `close(&self, id)`, `rename(&self, id, label)`, `remove(&self, id)`, `repair(&self, id) -> AppResult<RepairReport>`, `list(&self) -> AppResult<Vec<AccountView>>`.
- `pub struct AccountView { pub account: Account, pub state: AccountState, pub bytes: Option<u64> }`; `pub enum AccountState { Running, Stopped, Opening, Problem }`.

- [ ] **Step 1: Tes gagal** (dengan `FakePlatform` yang mencatat panggilan dan bisa disetel gagal per langkah):
```rust
#[test] fn add_happy_path_order() { /* urutan: vault_put, elevated(CreateUser), grant, bootstrap, store.save, launch_as */ }
#[test] fn add_rolls_back_when_bootstrap_fails() { /* bootstrap Err -> revoke, elevated(DeleteUser), vault_delete; store tidak berubah */ }
#[test] fn add_uac_cancel_before_user_created() { /* elevated Err(UacCancelled) -> vault_delete saja, tidak ada DeleteUser */ }
#[test] fn open_running_focuses() { /* running punya owner tsb -> Focused, launch tidak dipanggil */ }
#[test] fn open_low_ram_requires_confirm() { /* free_ram 1.5GB -> NeedsLowRamConfirm; confirmed=true -> Launched */ }
#[test] fn open_main_uses_launch_main() {}
#[test] fn remove_main_is_rejected() {}
#[test] fn remove_order() { /* close jika jalan, elevated(DeleteUser), remove_profile, revoke, vault_delete, store tanpa akun */ }
#[test] fn repair_logon_failure_resets_password() { /* launch_as Err(LogonFailed) -> elevated(SetPassword) + vault_put */ }
#[test] fn repair_rebootstraps_broken_junction() {}
```
- [ ] **Step 2:** `cargo test flows` → FAIL.
- [ ] **Step 3:** Implementasi.
- [ ] **Step 4:** `cargo test flows` → PASS.
- [ ] **Step 5:** Commit `feat: account flows over platform trait`.

### Task 9: commands + tray + logging + autostart

**Files:** Create `src-tauri/src/commands.rs`, `src-tauri/src/tray.rs`; Modify `src-tauri/src/main.rs`.

**Interfaces — Produces (perintah Tauri, nama persis dipakai UI):** `list_accounts() -> Vec<AccountView>`, `add_account(label) -> Account`, `open_account(id, confirmed_low_ram: bool) -> OpenOutcome`, `close_account(id)`, `rename_account(id, label)`, `remove_account(id)`, `repair_account(id) -> RepairReport`, `get_settings() -> { autostart: bool, free_ram: u64 }`, `set_autostart(enabled: bool)`, `hide_panel()`. Event: `status`, `progress` (`{ step: "vault"|"create_user"|"grant"|"bootstrap"|"launch", account_label }`).

- [ ] **Step 1:** `commands.rs` hanya meneruskan ke `Flows<WinPlatform>` dalam `tauri::State`, operasi lambat di `tauri::async_runtime::spawn_blocking`.
- [ ] **Step 2:** `tray.rs`: ikon dari `icons/tray-light.ico`/`tray-dark.ico` mengikuti tema (`AppsUseLightTheme` di registry), tooltip `Claude Switcher — {n} berjalan`; klik kiri → posisikan panel di atas ikon (pakai `TrayIconEvent::Click { rect }`, jepit ke work area monitor tersebut) lalu tampil+fokus; panel tersembunyi saat `WindowEvent::Focused(false)`; klik kanan → menu native (akun dengan ✓ jika berjalan, "Tambah akun…", "Keluar"); menu dibangun ulang saat event `status` berubah.
- [ ] **Step 3:** Logging `tracing` + `tracing-appender` ke path & rotasi di Global Constraints; tes unit `redacts_secrets` memastikan formatter tidak pernah mencetak field bernama `password`/`secret`.
- [ ] **Step 4:** `cargo test` → PASS; `cargo tauri dev` → ikon tray muncul, klik kiri membuka panel placeholder, klik kanan menampilkan "Utama".
- [ ] **Step 5:** Commit `feat: tauri commands, tray, logging, autostart`.

### Task 10: UI panel

**REQUIRED SUB-SKILL:** `design-taste-frontend` untuk arah visual (layar aplikasi), dalam batas spec §5.

**Files:** Create `ui/index.html`, `ui/styles.css`, `ui/app.js`, `ui/strings.js`, `ui/mock-invoke.js`.

**Interfaces — Consumes:** perintah & event Task 9. `app.js` memakai `window.__TAURI__.core.invoke` bila ada, selain itu `mock-invoke.js` (dipilih via `?mock=1`; mock menyediakan 3 akun: Utama berjalan 1,4 GB, Kantor mati, Klien bermasalah; dan bisa memicu error lewat `?fail=add`).

- [ ] **Step 1:** Struktur & perilaku sesuai spec §5 (Panel, Aksesibilitas): `h1` "Akun Claude"; `ul` akun; tiap `li` berisi nama, status teks, tombol utama (`Buka`/`Tampilkan`, `aria-describedby` → status), menu button `aria-haspopup="menu"` berlabel "Opsi untuk {label}"; form tambah inline dengan `label` terlihat; satu `div aria-live="polite"` untuk status/progres; error `role="alert"`; dialog konfirmasi hapus & RAM rendah memakai `<dialog>` modal dengan fokus awal di tombol Batal.
- [ ] **Step 2:** Keyboard: panah atas/bawah memindah fokus antar tombol utama baris; Esc menutup menu → dialog → panel (`hide_panel`); fokus kembali ke pemicu.
- [ ] **Step 3:** CSS: token warna terang/gelap via `prefers-color-scheme` yang lolos kontras Global Constraints; `@media (forced-colors: active)` memakai `CanvasText/ButtonText/Highlight`; `prefers-reduced-motion` → tanpa transisi; target ≥24px; fokus `outline: 2px solid` + offset 2px; font `"Segoe UI Variable", "Segoe UI", system-ui`; satuan `rem` agar ikut text scaling.
- [ ] **Step 4:** Buka `ui/index.html?mock=1` di browser, lakukan alur tambah/buka/hapus dengan keyboard saja → semua berfungsi.
- [ ] **Step 5:** Commit `feat: accessible tray panel ui`.

### Task 11: UI tests (Playwright + axe)

**Files:** Create `tests-ui/package.json`, `tests-ui/playwright.config.ts`, `tests-ui/panel.spec.ts`.

- [ ] **Step 1: Tes**
```ts
for (const scheme of ['light','dark'] as const) test(`axe AA ${scheme}`, async ({ page }) => {
  await page.emulateMedia({ colorScheme: scheme }); await page.goto(PANEL + '?mock=1');
  const r = await new AxeBuilder({ page }).withTags(['wcag2a','wcag2aa','wcag21aa','wcag22aa']).analyze();
  expect(r.violations).toEqual([]);
});
test('axe forced-colors', ...emulateMedia({ forcedColors: 'active' }) ...);
test('keyboard: arrow moves between rows, Enter opens', ...);
test('escape closes menu then returns focus to its button', ...);
test('add account announces progress in live region', ...expect(live).toHaveText(/Membuat user Windows/) ...);
test('add failure shows role=alert and keeps form values', ... '?mock=1&fail=add' ...);
test('remove asks confirmation naming the account', ...expect(dialog).toContainText('Kantor') ...);
test('status is text, not color only', ...expect(row).toContainText('Berjalan') ...);
test('targets at least 24px', ...boundingBox().height >= 24 untuk semua button...);
test('200% text zoom does not clip labels', ...);
```
- [ ] **Step 2:** `cd tests-ui && npm i && npx playwright install chromium && npx playwright test` → semua PASS (perbaiki UI, bukan tesnya, bila gagal).
- [ ] **Step 3:** Commit `test: playwright + axe accessibility suite`.

### Task 12: Manual integration checklist + release build

**Files:** Create `tests/manual/README.md`; Modify `src-tauri/tauri.conf.json` (bundle NSIS, per-user install).

- [ ] **Step 1:** Tulis checklist: (1) tambah akun "Tes Integrasi" → 1 UAC, Claude terbuka, login; (2) `whoami` di Claude Code akun itu ≠ alice & skill pribadi terlihat; (3) buka Utama + 2 akun bersamaan, panel menampilkan RAM tiap akun; (4) ubah password user via `net user` → status Bermasalah → Perbaiki → buka lagi OK; (5) hapus akun → `net user` tidak ada, `C:\Users\claude-tes-integrasi` hilang, `~/.claude` utama utuh, `cmdkey /list` tanpa target `ClaudeSwitcher/...`, `icacls ~/.claude` tanpa ACE user itu; (6) Narrator: semua kontrol terbaca dengan nama & status.
- [ ] **Step 2:** `cargo tauri build` → installer di `src-tauri/target/release/bundle/nsis/`.
- [ ] **Step 3:** Jalankan checklist; catat hasil di README.
- [ ] **Step 4:** Commit `docs: manual integration checklist; build config`.
