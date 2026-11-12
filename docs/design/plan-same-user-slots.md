# Same-User Slots Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Saat **Tambah akun**, pengguna memilih "User Windows ini" (slot dengan `LOCALAPPDATA` sendiri, tanpa UAC, jumlahnya tidak terbatas) atau "User Windows terpisah" (alur yang sudah ada).

**Architecture:** Slot adalah folder `%LOCALAPPDATA%\ClaudeSwitcher\slots\<account-id>\Local` yang berisi `Claude-Data\claude_<family>` (dibuat lebih dulu) dan junction `Microsoft\WindowsApps` ke folder aslinya. Claude dijalankan lewat alias asli dengan env `LOCALAPPDATA` diarahkan ke slot. Monitor membedakan instance di SID yang sama lewat `--user-data-dir` pada baris perintah proses anak. Operasi slot ditambahkan ke trait `Platform`, jadi `flows` tetap bisa diuji dengan `Fake`.

**Tech Stack:** Rust stable, Tauri 2, `windows` crate (sudah dipakai), UI HTML/JS polos, Playwright + axe.

**Spec:** `docs/design/same-user-slots.md` (melengkapi `docs/design/design.md`)

## Global Constraints

- Semua Global Constraints di `docs/design/plan-v1.md` tetap berlaku.
- Root slot: `%LOCALAPPDATA%\ClaudeSwitcher\slots\<account-id>\Local`. Folder dinamai dengan `id`, bukan label.
- Folder data slot: `<data_root>\Claude-Data\<family huruf kecil>`, contohnya `claude_pzs8sxrjxfjjc`.
- Junction slot: `<data_root>\Microsoft\WindowsApps` → `%LOCALAPPDATA%\Microsoft\WindowsApps` yang asli.
- Log Claude milik slot: `<data_root>\Claude\logs\main.log`. Label sehat diawali `already_moved`.
- Mode slot **tidak** memanggil UAC, vault, `grant`/`revoke`, maupun `elevated`.
- `accounts.json` tetap `version: 1`. `kind` baru `"same_user"`. Field `data_root` opsional dan tidak ditulis bila `None`.
- Default radio di UI: **User Windows ini**.
- Copy UI (persis):
  - legend: "Jalankan di"
  - opsi 1: "User Windows ini", petunjuk "Cepat, tanpa izin admin. Berbagi pengaturan Windows dengan akun Utama."
  - opsi 2: "User Windows terpisah", petunjuk "Terisolasi penuh. Perlu izin admin (UAC) sekali."
  - teks jenis di baris akun: "User ini" / "User terpisah"
  - error: "Gagal menyiapkan folder akun."

## Review Focus

1. **Hapus slot tidak boleh menembus junction `Microsoft\WindowsApps`.** Kalau terjadi, alias Claude asli ikut terhapus. Junction dilepas sebagai link sebelum folder dihapus. Tes `remove_keeps_junction_target` di Task 3.
2. **Utama tetap terdeteksi saat ada slot.** Semua instance memakai SID yang sama, jadi Utama = instance SID utama yang `data_dir`-nya bukan milik slot mana pun, termasuk saat `data_dir` = `None`. Tes `main_is_the_instance_no_slot_claims` di Task 4.
3. **Path berbeda huruf besar/kecil atau garis miring** (`C:\Users\X\…` vs `c:/users/x/…`, ada `\` di akhir) tetap dianggap sama. Tes `data_dir_match_ignores_case_and_separators` di Task 2.
4. **Folder slot dihapus di luar app** → Buka membuat ulang folder dan junction, lalu meluncurkan. Tes `open_same_user_recreates_missing_slot` di Task 4.
5. **Gagal di tengah Tambah** (folder gagal dibuat, atau gagal simpan) → tidak ada akun tersimpan dan tidak ada folder tersisa. Tes `add_same_user_rolls_back_folder_when_save_fails` di Task 4.

---

## File Structure

| File | Perubahan |
|---|---|
| `src-tauri/src/store.rs` | `AccountKind::SameUser`, field `Account::data_root` |
| `src-tauri/src/monitor.rs` | `ProcInfo::cmdline`, `InstanceStatus::data_dir`, parser `--user-data-dir`, pencocokan path |
| `src-tauri/src/slot.rs` (baru) | path slot, siapkan/cek/hapus folder, luncurkan, label migrasi dari log, impor riwayat |
| `src-tauri/src/platform.rs` | method slot di trait `Platform` + implementasi `WinPlatform` |
| `src-tauri/src/flows.rs` | `AddMode`, cabang `SameUser` di semua alur, `Fake` diperluas |
| `src-tauri/src/commands.rs`, `src-tauri/src/lib.rs` | `add_account` menerima `mode` |
| `ui/index.html`, `ui/app.js`, `ui/strings.js`, `ui/styles.css`, `ui/mock-invoke.js` | radio group, teks jenis, copy |
| `tests-ui/panel.spec.ts` | tes radio group dan copy per mode |
| `tests/manual/README.md`, spec utama §2 | langkah S1–S8, catatan temuan |

---

### Task 1: Model akun `SameUser`

**Files:**
- Modify: `src-tauri/src/store.rs`
- Test: `src-tauri/src/store.rs` (modul `tests`)

**Interfaces:**
- Produces: `AccountKind::SameUser` (serde `"same_user"`); `Account { …, data_root: Option<PathBuf> }` dengan `#[serde(default, skip_serializing_if = "Option::is_none")]`. `Account::main()` mengisi `data_root: None`.

- [ ] **Step 1: Tes gagal**

```rust
#[test]
fn same_user_roundtrips_with_data_root() {
    let dir = tmp();
    let s = Store::new(dir.join("accounts.json"));
    let mut a = acct("Slot A", AccountKind::SameUser, None);
    a.data_root = Some(PathBuf::from(r"C:\x\slots\id\Local"));
    s.save(&[a.clone()]).unwrap();
    assert_eq!(s.load().unwrap(), LoadOutcome::Ok(vec![a]));
    let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(dir.join("accounts.json")).unwrap()).unwrap();
    assert_eq!(v["accounts"][0]["kind"], "same_user");
}

#[test]
fn old_file_without_data_root_still_loads() {
    let dir = tmp();
    std::fs::write(dir.join("accounts.json"), r#"{"version":1,"accounts":[{"id":"1","label":"Utama","kind":"main","windows_user":null,"created_at":"x"}]}"#).unwrap();
    let LoadOutcome::Ok(list) = Store::new(dir.join("accounts.json")).load().unwrap() else { panic!() };
    assert_eq!(list[0].data_root, None);
}

#[test]
fn windows_user_account_does_not_write_data_root() {
    let dir = tmp();
    let s = Store::new(dir.join("accounts.json"));
    s.save(&[acct("Kantor", AccountKind::WindowsUser, Some("claude-kantor"))]).unwrap();
    assert!(!std::fs::read_to_string(dir.join("accounts.json")).unwrap().contains("data_root"));
}
```

Perbarui helper `acct(..)` dan semua literal `Account { .. }` di crate (termasuk `flows.rs`) dengan `data_root: None`.

- [ ] **Step 2:** `cargo test store` → FAIL (varian/field belum ada).
- [ ] **Step 3:** Tambahkan varian dan field. Semua `match` atas `AccountKind` yang belum lengkap boleh `todo!()` sementara; Task 4 mengisinya.
- [ ] **Step 4:** `cargo test store` → PASS.
- [ ] **Step 5:** Commit `feat: same_user account kind with data_root`.

---

### Task 2: Monitor mengenali folder data tiap instance

**Files:**
- Modify: `src-tauri/src/monitor.rs`

**Interfaces:**
- Produces:
  - `ProcInfo { …, cmdline: Option<String> }`: `None` untuk proses user lain atau yang tidak terbaca.
  - `InstanceStatus { owner_sid, root_pid, bytes, data_dir: Option<PathBuf> }`.
  - `pub fn user_data_dir_arg(cmdline: &str) -> Option<PathBuf>`.
  - `pub fn same_dir(a: &Path, b: &Path) -> bool`.
- `data_dir` sebuah instance diambil dari keturunan pertama dalam pohonnya yang punya `--user-data-dir`. Proses root Claude sendiri tidak membawa argumen itu; proses anaknya membawa.

- [ ] **Step 1: Tes gagal**

```rust
#[test]
fn parses_user_data_dir_quoted_and_bare() {
    assert_eq!(user_data_dir_arg(r#""C:\…\Claude.exe" --type=renderer --user-data-dir="C:\Users\a b\Local\Claude-Data\claude_x" --x"#),
               Some(PathBuf::from(r"C:\Users\a b\Local\Claude-Data\claude_x")));
    assert_eq!(user_data_dir_arg(r"Claude.exe --user-data-dir=C:\d\claude_x --y"), Some(PathBuf::from(r"C:\d\claude_x")));
    assert_eq!(user_data_dir_arg("Claude.exe --type=gpu-process"), None);
}

#[test]
fn data_dir_match_ignores_case_and_separators() {
    assert!(same_dir(Path::new(r"C:\Users\X\Local\Claude-Data\claude_p\"), Path::new("c:/users/x/local/claude-data/claude_p")));
    assert!(!same_dir(Path::new(r"C:\a\claude_p"), Path::new(r"C:\a\claude_p2")));
}

#[test]
fn same_sid_instances_are_split_by_data_dir() {
    // three roots under one SID; each has a child with its own --user-data-dir
    let procs = vec![
        claude(10, 1, "S", 100), child(11, 10, "S", r"--user-data-dir=C:\main\claude_p"),
        claude(20, 1, "S", 100), child(21, 20, "S", r"--user-data-dir=C:\slotA\claude_p"),
        claude(30, 1, "S", 100), child(31, 30, "S", r"--user-data-dir=C:\slotB\claude_p"),
    ];
    let mut got: Vec<_> = instances(&procs, Path::new(INSTALL)).into_iter().map(|i| (i.root_pid, i.data_dir)).collect();
    got.sort();
    assert_eq!(got, vec![
        (10, Some(PathBuf::from(r"C:\main\claude_p"))),
        (20, Some(PathBuf::from(r"C:\slotA\claude_p"))),
        (30, Some(PathBuf::from(r"C:\slotB\claude_p"))),
    ]);
}
```

`child(pid, parent, sid, cmdline)` adalah helper tes baru: `Claude.exe` di paket dengan `cmdline: Some(..)`. Perbarui `claude(..)` dengan `cmdline: None`. `key()` di `changed()` ikut memasukkan `data_dir`.

- [ ] **Step 2:** `cargo test monitor` → FAIL.
- [ ] **Step 3:** Implementasi. Baca baris perintah di `processes()` hanya untuk proses `claude.exe` milik SID sendiri, dengan `OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION)` lalu `NtQueryInformationProcess(ProcessCommandLineInformation = 60)` (`UNICODE_STRING` di buffer). Kalau gagal → `None`, tanpa error. `same_dir` membandingkan path setelah `/`→`\`, buang `\` di akhir, lalu `to_lowercase`.
- [ ] **Step 4:** `cargo test monitor` → PASS. Tes integrasi `snapshot_sees_this_user` yang sudah ada tetap lolos.
- [ ] **Step 5:** Commit `feat: monitor tells same-user instances apart by user-data-dir`.

---

### Task 3: Modul `slot` dan method `Platform`

**Files:**
- Create: `src-tauri/src/slot.rs` (daftarkan di `lib.rs`)
- Modify: `src-tauri/src/platform.rs`

**Interfaces:**
- Consumes: `claude_pkg::ClaudePkg { family, .. }`, `history::{sessions_dirs, import_plan, copy_plan}`, `share::is_junction_to`.
- Produces (`slot.rs`):
  - `pub fn root_for(slots_dir: &Path, account_id: &str) -> PathBuf` → `<slots_dir>\<id>\Local`
  - `pub fn data_dir(data_root: &Path, family: &str) -> PathBuf` → `<data_root>\Claude-Data\<family lowercase>`
  - `pub fn prepare(data_root: &Path, family: &str, real_windowsapps: &Path) -> AppResult<()>` (idempotent)
  - `pub fn is_ready(data_root: &Path, family: &str, real_windowsapps: &Path) -> bool`
  - `pub fn remove(slot_dir: &Path) -> AppResult<()>`, dengan `slot_dir` = induk `data_root` (`<slots_dir>\<id>`)
  - `pub fn launch(data_root: &Path, real_alias: &Path) -> AppResult<()>`
  - `pub fn move_label(data_root: &Path) -> Option<String>`: label `[user-data-move] <label>` **terakhir** di `<data_root>\Claude\logs\main.log`
  - `pub fn import_sessions(real_local_appdata: &Path, data_root: &Path, family: &str) -> AppResult<usize>`
- Produces (trait `Platform`, nama persis):

```rust
fn slots_dir(&self) -> PathBuf;
fn slot_data_dir(&self, data_root: &Path) -> PathBuf;
fn slot_prepare(&self, data_root: &Path) -> AppResult<()>;
fn slot_ready(&self, data_root: &Path) -> bool;
fn slot_remove(&self, data_root: &Path) -> AppResult<()>;
fn launch_slot(&self, data_root: &Path) -> AppResult<()>;
fn slot_move_label(&self, data_root: &Path) -> Option<String>;
fn slot_import(&self, data_root: &Path) -> AppResult<usize>;
```

- [ ] **Step 1: Tes gagal** (di filesystem nyata, folder temp)

```rust
#[test]
fn prepare_makes_data_dir_and_junction_and_is_idempotent() {
    let (root, real_apps) = (tmp().join("Local"), tmp_with_file("claude-desktop.exe"));
    prepare(&root, "Claude_PZS", &real_apps).unwrap();
    prepare(&root, "Claude_PZS", &real_apps).unwrap();
    assert!(root.join(r"Claude-Data\claude_pzs").is_dir());
    assert!(crate::share::is_junction_to(&root.join(r"Microsoft\WindowsApps"), &real_apps));
    assert!(is_ready(&root, "Claude_PZS", &real_apps));
}

#[test]
fn remove_keeps_junction_target() {
    let (root, real_apps) = (tmp().join("Local"), tmp_with_file("claude-desktop.exe"));
    prepare(&root, "Claude_PZS", &real_apps).unwrap();
    remove(root.parent().unwrap()).unwrap();
    assert!(!root.exists());
    assert!(real_apps.join("claude-desktop.exe").exists());
}

#[test]
fn move_label_reads_the_last_label() {
    let root = tmp().join("Local");
    write(root.join(r"Claude\logs\main.log"), "… [user-data-move] stay_never_left {…}\n… [user-data-move] already_moved_package_emptied { blocked: null }\n");
    assert_eq!(move_label(&root).as_deref(), Some("already_moved_package_emptied"));
    assert_eq!(move_label(&tmp()), None);
}

#[test]
fn import_copies_sessions_without_overwriting() {
    // real: <real>\Claude-Data\claude_pzs\claude-code-sessions\acct\a.json + b.json
    // slot: <root>\Claude-Data\claude_pzs\claude-code-sessions\acct\a.json (different content)
    // → returns 1; a.json in the slot unchanged; b.json copied
}
```

Tulis tes keempat lengkap dengan pola `history.rs::copy_plan_copies_and_never_overwrites`.

- [ ] **Step 2:** `cargo test slot` → FAIL.
- [ ] **Step 3:** Implementasi.
  - Junction dibuat dengan `cmd /c mklink /J <link> <target>` (tanpa admin). Kalau folder biasa sudah ada di posisi link dan kosong, folder itu dihapus dulu.
  - Di `remove`, setiap junction di bawah `slot_dir` dilepas dengan `std::fs::remove_dir` (bukan `remove_dir_all`) **sebelum** `remove_dir_all(slot_dir)`. Contohnya ada di `share::remove_profile_safely`.
  - `launch` menjalankan `Command::new(real_alias).env("LOCALAPPDATA", data_root).spawn()`. Kalau alias tidak ada → `ErrorCode::ClaudeMissing`.
  - `import_sessions` memakai `history::sessions_dirs(real_local_appdata)` sebagai sumber, `data_dir(..)\claude-code-sessions` sebagai tujuan, lalu `import_plan` + `copy_plan`. Kalau tujuan tanpa folder akun → `ErrorCode::NotFound` dengan `"err.import_no_signin"`, sama seperti alur elevated.
  - `WinPlatform` meneruskan ke fungsi-fungsi ini dengan `pkg.family`, `%LOCALAPPDATA%\Microsoft\WindowsApps`, dan alias `…\WindowsApps\claude-desktop.exe` asli. `slots_dir()` = `%LOCALAPPDATA%\ClaudeSwitcher\slots`.
- [ ] **Step 4:** `cargo test slot` → PASS.
- [ ] **Step 5:** Commit `feat: same-user slot folders, launch and history import`.

---

### Task 4: Alur `SameUser` di `flows`

**Files:**
- Modify: `src-tauri/src/flows.rs` (termasuk `Fake` di modul tes)

**Interfaces:**
- Consumes: method slot `Platform` dari Task 3; `InstanceStatus::data_dir`, `monitor::same_dir` dari Task 2.
- Produces:
  - `#[derive(Deserialize)] #[serde(rename_all = "snake_case")] pub enum AddMode { SameUser, WindowsUser }`
  - `pub fn add(&self, label: &str, mode: AddMode, progress: &dyn Fn(&str)) -> AppResult<Account>`. Alur `WindowsUser` tidak berubah.
  - Kunci progres slot: `"prepare_slot"`, lalu `"launch"`.
  - Varian baru `OpenOutcome::SlotRecreated` (serde snake_case): folder slot hilang, lalu dibuat ulang dan diluncurkan. UI mengumumkannya di live region (Task 5).
  - `WinPlatform::slot_remove(data_root)` memanggil `slot::remove(data_root.parent())`.
- `Fake` mencatat `slot_prepare <root>`, `slot_remove <root>`, `launch_slot <root>`, dan `slot_import <root>`; bisa disetel gagal per langkah lewat `check(..)`. `slots_dir()` = `C:\slots`. `slot_data_dir(r)` = `r\Claude-Data\claude_p`. `slot_ready` membaca set `slot_missing`. `slot_move_label` membaca map yang bisa disetel (default `already_moved_package_emptied`).

- [ ] **Step 1: Tes gagal**

```rust
#[test] fn add_same_user_prepares_saves_launches_without_uac() {
    // calls == ["slot_prepare C:\slots\<id>\Local", "launch_slot C:\slots\<id>\Local"]
    // no vault_*/create_user/grant/bootstrap; account.kind == SameUser; data_root == Some(that path); windows_user None
}
#[test] fn add_same_user_prepare_failure_saves_nothing() { /* fail "slot_prepare" → Err; accounts() == [main] */ }
#[test] fn add_same_user_rolls_back_folder_when_save_fails() { /* store path read-only/dir → Err; calls end with "slot_remove …" */ }
#[test] fn open_same_user_launches_slot() { /* calls contain "launch_slot <root>"; OpenOutcome::Launched */ }
#[test] fn open_same_user_recreates_missing_slot() { /* slot_missing contains root → "slot_prepare" before "launch_slot"; returns OpenOutcome::SlotRecreated */ }
#[test] fn main_is_the_instance_no_slot_claims() {
    // running: {SID-main, pid 1, data_dir Some(main)}, {SID-main, pid 2, data_dir Some(<slot>\Claude-Data\claude_p)}, {SID-main, pid 3, data_dir None}
    // list(): Utama Running with pid 1 or 3 bytes (not 2); slot Running with pid 2 bytes
}
#[test] fn close_same_user_kills_its_root_as_main() { /* calls contain "close 2 main" */ }
#[test] fn remove_same_user_closes_then_deletes_folder() { /* ["close 2 main", "slot_remove <root>"]; no revoke/delete_user/vault */ }
#[test] fn import_same_user_closes_then_copies_without_elevation() { /* "slot_import <root>"; no "import_sessions" */ }
#[test] fn repair_same_user_reprepares_slot() { /* "slot_prepare <root>"; problem cleared */ }
#[test] fn unexpected_move_label_marks_problem() { /* label "stay_never_left" while running → AccountState::Problem */ }
```

Tulis assert-nya sebagai kode lengkap dengan helper yang sudah ada (`flows()`, `main_id()`, `f.p.calls()`).

- [ ] **Step 2:** `cargo test flows` → FAIL.
- [ ] **Step 3:** Implementasi.
  - `sid()` untuk `SameUser` = `main_sid()`.
  - `running_root` untuk `SameUser` cocok bila `same_dir(i.data_dir, slot_data_dir(root))`. Untuk `Main`, cocok dengan instance SID utama yang `data_dir`-nya tidak cocok dengan slot mana pun (ambil yang pertama).
  - `id` akun dibuat sebelum `slot_prepare`, karena path slot memakai id itu.
  - `list()` menandai Problem untuk `SameUser` yang berjalan bila `slot_move_label` ada dan tidak diawali `already_moved`.
  - `close`, `remove`, dan `import_history` untuk `SameUser` memanggil `p.close(pid, None)`.
- [ ] **Step 4:** `cargo test` → semua PASS.
- [ ] **Step 5:** Commit `feat: same-user account flows`.

---

### Task 5: Command dan UI pilihan mode

**Files:**
- Modify: `src-tauri/src/commands.rs` (`add_account(app, state, label: String, mode: AddMode)`), `ui/index.html`, `ui/app.js`, `ui/strings.js`, `ui/styles.css`, `ui/mock-invoke.js`
- Test: `tests-ui/panel.spec.ts`

**Interfaces:**
- Consumes: `AddMode` (JSON `"same_user"` / `"windows_user"`), `Account.kind`.
- Produces: `invoke("add_account", { label, mode })`. Mock `add_account({label, mode})` mengirim progres `prepare_slot` → `launch` untuk `same_user`, dan urutan lama untuk `windows_user`. Mock menyimpan `kind` sesuai mode.

- [ ] **Step 1: Tes gagal**

```ts
test('add form offers both modes, "User Windows ini" selected by default', async ({ page }) => {
  await page.goto('/?mock=1'); await page.getByRole('button', { name: 'Tambah akun' }).click();
  const group = page.getByRole('group', { name: 'Jalankan di' });
  await expect(group.getByRole('radio', { name: 'User Windows ini' })).toBeChecked();
  await expect(group.getByRole('radio', { name: 'User Windows terpisah' })).not.toBeChecked();
});
test('arrow keys switch mode and the hint follows', async ({ page }) => { /* focus radio 1, ArrowDown → radio 2 checked; hint text contains "Perlu izin admin" */ });
test('same-user add never mentions the admin prompt', async ({ page }) => { /* live region texts: none match /admin|UAC/ */ });
test('rows say which kind they are', async ({ page }) => { /* added same-user row has text "User ini"; windows_user row "User terpisah"; primary button aria-describedby includes it */ });
test('import on a same-user account does not mention admin', async ({ page }) => { /* dialog body has no /admin/ */ });
test('opening a slot whose folder was deleted says so', async ({ page }) => { /* ?mock=1&slot=missing → live region contains "dibuat ulang" */ });
```

Jalankan juga ulang semua tes axe yang ada, dengan form tambah terbuka.

- [ ] **Step 2:** `cd tests-ui && npx playwright test` → FAIL.
- [ ] **Step 3:** Implementasi.
  - `fieldset` + `legend` "Jalankan di", dua `input type=radio name=mode` dengan `label` terlihat. Petunjuk mode memakai `aria-describedby` yang diperbarui saat pilihan berubah.
  - Hint lama `add_hint` dipecah: bagian "Windows akan meminta persetujuan admin sekali." pindah ke petunjuk opsi 2.
  - String baru: `mode_legend`, `mode_same`, `mode_same_hint`, `mode_windows`, `mode_windows_hint`, `kind_same`, `kind_windows`, `progress_prepare_slot` ("Menyiapkan folder akun…"), `import_body_same` (salinan `import_body` tanpa kalimat admin), `remove_body_same` ("Login dan data Claude di akun ini ikut terhapus. Akun Utama tidak terpengaruh."), `slot_recreated` ("Folder akun {label} dibuat ulang. Login lagi di jendela Claude yang terbuka."), dan `"err.slot_prepare"` = "Gagal menyiapkan folder akun.". `openAccount` mengumumkan `slot_recreated` bila hasilnya `slot_recreated`; mock mendukung `?slot=missing` untuk tesnya.
  - Target radio ≥24px; fokus dan forced-colors mengikuti token yang ada.
- [ ] **Step 4:** `npx playwright test` → semua PASS. `cargo test` → PASS.
- [ ] **Step 5:** Commit `feat: choose same or separate Windows user when adding an account`.

---

### Task 6: Uji manual, dokumen, dan build

**Files:**
- Modify: `tests/manual/README.md`, `docs/design/design.md` (§2)

- [ ] **Step 1:** Tambahkan bagian "Akun di user Windows yang sama" ke checklist manual, dengan tabel langkah **S1–S8** persis dari spec slot §8. Tambahkan juga baris di §2 spec utama: "Override `LOCALAPPDATA` + `Claude-Data\claude_<family>` yang dibuat lebih dulu → banyak slot (diuji 2026-11-12, Claude 5.1231.2.0); lihat `same-user-slots.md`."
- [ ] **Step 2:** `cargo tauri build` → installer di `src-tauri/target/release/bundle/nsis/`.
- [ ] **Step 3:** Jalankan S1–S8 bersama pengguna dan isi kolom hasil. Kalau S3 (login bersamaan) atau S7 (Cowork) gagal, catat di bagian "Hasil" dan tampilkan peringatan di petunjuk opsi 1. Jangan menghapus mode ini tanpa persetujuan pengguna.
- [ ] **Step 4:** Commit `docs: same-user slot checklist and findings`.
