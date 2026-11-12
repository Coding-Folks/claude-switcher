# Claude Switcher — Desain

Tanggal: 2026-11-12 · Status: menunggu review

## 1. Tujuan

Aplikasi tray Windows (Rust, Tauri v2) untuk menjalankan **banyak akun Claude Desktop bersamaan** dan menambah akun dengan mudah. Semua akun berbagi skills, plugins, memory, dan riwayat Claude Code (`~/.claude`) milik user utama.

**Kriteria sukses**
- Menambah akun baru: satu alur, satu prompt UAC, tanpa mengetik password Windows.
- Membuka akun: satu klik dari tray.
- Jumlah akun yang berjalan bersamaan hanya dibatasi RAM.
- Skills, plugins, dan memory yang dipasang di akun mana pun terlihat di semua akun.
- UI memenuhi WCAG 2.2 AA dan bisa dipakai penuh dengan keyboard serta Narrator/NVDA.

**Bukan tujuan versi 1:** impor riwayat sesi otomatis, hotkey global, sinkronisasi antar komputer, dukungan macOS/Linux.

## 2. Temuan yang mendasari desain

Diuji di mesin target (Windows 11, Claude Desktop MSIX 5.1231.2.0):

| Uji | Hasil |
|---|---|
| `--user-data-dir` / override `APPDATA` | Diabaikan; instance kedua diteruskan ke instance yang sudah ada. |
| Override `LOCALAPPDATA` | Berhasil, tetapi hanya memberi **1** slot tambahan (jatuh ke profil lama `LocalCache\Roaming\Claude`). |
| `CLAUDE_USER_DATA_DIR` | Ada di kode, tetapi dinonaktifkan di build rilis (proses langsung keluar). |
| Sandboxie-Plus | Claude tidak jalan di sandbox (kemungkinan karena MSIX tidak didukung). |
| **User Windows terpisah + junction `~/.claude`** | **Berhasil**: instance ketiga jalan bersamaan, login OK, skills terbaca. |
| Override `LOCALAPPDATA` + `Claude-Data\claude_<family>` yang dibuat lebih dulu | **Banyak slot** di user Windows yang sama (diuji 2026-11-12, Claude 5.1231.2.0); lihat `2026-11-12-same-user-slots-design.md`. Baris "Override `LOCALAPPDATA`" di atas hanya berlaku bila folder itu tidak disiapkan. |

Kesimpulan: **satu akun Claude = satu user Windows lokal tersembunyi.**

## 3. Arsitektur

Satu executable `claude-switcher.exe` yang berjalan sebagai user utama (tanpa admin). Operasi yang butuh admin dijalankan oleh exe yang sama dengan argumen `--elevated <op> <args-json>` melalui `ShellExecuteExW` verb `runas` (UAC). Proses elevated tidak punya UI, menulis hasil JSON ke stdout-file sementara, lalu keluar.

```
claude-switcher/
  src-tauri/src/
    main.rs        // titik masuk: mode tray atau --elevated
    store.rs       // accounts.json
    vault.rs       // Windows Credential Manager
    winuser.rs     // buat/hapus/sembunyikan user (elevated)
    share.rs       // ACL ~/.claude + junction
    launcher.rs    // jalankan Claude sebagai user lain + bootstrap
    monitor.rs     // status proses & RAM
    commands.rs    // perintah Tauri yang dipanggil UI
    tray.rs        // ikon tray, menu klik kanan, posisi panel
  ui/              // HTML/CSS/JS panel (tanpa framework)
```

### Unit dan antarmukanya

| Unit | Tanggung jawab | Bergantung pada |
|---|---|---|
| `store` | Baca/tulis `%APPDATA%\ClaudeSwitcher\accounts.json` secara atomik (tulis `.tmp` lalu rename). Skema: `{ version: 1, accounts: [{ id, label, kind: "main" \| "windows_user", windows_user?, created_at }] }`. **Tidak pernah menyimpan password.** | fs |
| `vault` | `put(user, secret)`, `get(user)`, `delete(user)` via `CredWriteW`/`CredReadW`/`CredDeleteW`, target `ClaudeSwitcher/<windows_user>`, tipe `CRED_TYPE_GENERIC`, persist `LOCAL_MACHINE`. | `windows` crate |
| `winuser` | `create(name, password)` (`NetUserAdd`, flag `UF_DONT_EXPIRE_PASSWD`, grup Users), `hide(name)` (`HKLM\SOFTWARE\Microsoft\Windows NT\CurrentVersion\Winlogon\SpecialAccounts\UserList\<name> = 0`), `set_password`, `delete(name)` (`NetUserDel` + hapus nilai registry). Hanya dipanggil dalam mode elevated. | `windows` crate |
| `share` | `grant_shared_access(main_profile, user)`: tambah ACE Modify (OI)(CI) untuk `user` pada `<main>\.claude`. `junction_script(main_profile)`: skrip yang membuat `%USERPROFILE%\.claude` → `<main>\.claude`. `verify(user_profile)`. | `windows` crate |
| `launcher` | `bootstrap(user, password)`: jalankan PowerShell sebagai user (`CreateProcessWithLogonW`, `LOGON_WITH_PROFILE`) untuk `Add-AppxPackage -RegisterByFamilyName -MainPackage Claude_pzs8sxrjxfjjc` dan membuat junction, dengan hasil ditulis ke file log. `launch(account)`: akun `main` → jalankan alias `claude-desktop.exe` biasa; akun `windows_user` → `CreateProcessWithLogonW` ke `<profile>\AppData\Local\Microsoft\WindowsApps\claude-desktop.exe`. `focus(account)`: bawa jendela yang ada ke depan. `close(account)`: kirim WM_CLOSE ke jendela utama, lalu terminasi pohon proses setelah 10 detik bila belum keluar. | `vault`, `windows` crate |
| `monitor` | Setiap 5 detik: daftar proses `Claude.exe` (path WindowsApps, tanpa `--type=`), pemiliknya, dan jumlah working-set pohon prosesnya. Emit event `status` ke UI hanya bila berubah. Juga RAM bebas sistem. | `sysinfo` / `windows` crate |
| `commands` | `list_accounts`, `add_account(label)`, `open_account(id)`, `close_account(id)`, `rename_account(id, label)`, `remove_account(id)`, `repair_account(id)`, `get_settings`, `set_autostart(bool)`. Mengembalikan `Result<T, AppError>` dengan `AppError { code, message_id, detail }`. | semua di atas |

Nama package family (`Claude_pzs8sxrjxfjjc`) dan nama alias dibaca saat runtime dari `Get-AppxPackage`/registry, bukan hard-code, dengan nilai di atas sebagai fallback.

## 4. Alur

### Tambah akun
1. Pengguna mengisi label (misalnya "Kantor"). Nama user Windows dibuat dari label: `claude-<slug>`, maksimal 20 karakter, dengan sufiks angka bila sudah ada.
2. App membuat password acak 32 karakter (CSPRNG) dan menyimpannya ke `vault`.
3. **UAC** → elevated `create_user`: `winuser::create` + `winuser::hide`.
4. App (non-admin) memanggil `share::grant_shared_access`.
5. `launcher::bootstrap`: profil user terbentuk, Claude terdaftar, junction terpasang. Log dicek: register OK, junction menunjuk ke `~/.claude` utama.
6. Akun disimpan ke `store`, lalu `launcher::launch` → pengguna login di jendela Claude.

**Rollback:** jika langkah 3–5 gagal, app menghapus kredensial vault, ACE, dan (dengan UAC kedua, hanya jika user sudah sempat dibuat) user Windows-nya. Akun tidak masuk `store`. Pesan error menyebut langkah yang gagal.

### Buka akun
- Sudah berjalan → `focus`.
- Belum berjalan → jika RAM bebas < 2 GB, minta konfirmasi; lalu `launch`. Status "Membuka…" sampai `monitor` melihat prosesnya (batas 30 detik → error "Claude tidak terbuka").

### Hapus akun
Dialog konfirmasi menyebut nama akun dan bahwa user Windows serta data login Claude-nya akan dihapus permanen (skills/memory bersama tidak terpengaruh). Lalu `close` → **UAC** → `winuser::delete` + hapus folder profil `C:\Users\claude-<slug>` (junction dihapus sebagai link, **tidak** mengikuti target) → hapus ACE, kredensial, dan entri `store`.

### Perbaiki akun (`repair`)
Verifikasi junction, ACE, kredensial, dan registrasi Claude; perbaiki yang rusak. Jika password ditolak Windows (`ERROR_LOGON_FAILURE`), buat password baru → UAC `set_password` → simpan ke vault.

### Migrasi Akun 2 (sekali, manual)
Tambah akun baru lewat app dan login dengan akun ke-2. Shortcut lama `Claude (Akun 2)` dan skrip `Claude-Akun2.bat` dihapus oleh pengguna setelahnya. Impor riwayat sesi tidak otomatis di versi 1.

## 5. UI

### Tray
- Ikon monokrom yang mengikuti tema taskbar; tooltip: "Claude Switcher — N berjalan".
- **Klik kiri** → panel. **Klik kanan** → menu native: daftar akun (✓ jika berjalan; klik = buka/fokus), pemisah, "Tambah akun…", "Keluar".

### Panel
- Jendela tanpa bingkai, lebar 320px, tinggi sesuai isi (maksimal 480px dengan scroll), ditempatkan di atas area tray pada monitor yang berisi kursor. Tertutup saat kehilangan fokus atau Esc.
- Isi, berurutan:
  1. Judul "Akun Claude" (`h1`, visual kecil).
  2. Daftar akun (`ul`). Tiap baris: label; status teks ("Berjalan · 1,4 GB", "Mati", "Membuka…", "Bermasalah") dengan indikator bentuk/warna sebagai pelengkap; tombol utama **Buka** / **Tampilkan**; tombol menu "Opsi untuk <label>" (Tutup, Ganti nama, Perbaiki, Hapus).
  3. Tombol **Tambah akun** → form inline (label + Batal/Buat) dengan langkah progres yang diumumkan.
  4. Footer: "RAM bebas 9,2 GB" dan toggle "Jalan saat Windows mulai".
- Arah visual ditentukan saat implementasi dengan skill `design-taste-frontend` (anti-slop): tipografi sistem (Segoe UI Variable), palet netral dengan satu aksen, tanpa gradien/glassmorphism/emoji dekoratif, mengikuti tema terang/gelap Windows.

### Aksesibilitas (WCAG 2.2 AA)
- HTML semantik (`button`, `ul/li`, `form`, `label`); ARIA hanya bila tidak ada padanan native. Menu opsi memakai pola menu button WAI-ARIA.
- Keyboard: fokus pertama di akun pertama; Tab/Shift+Tab antar kontrol; panah atas/bawah antar baris; Enter/Spasi mengaktifkan; Esc menutup menu, lalu panel. Fokus kembali ke pemicu setelah menu/dialog ditutup. Tidak ada jebakan fokus selain di dialog konfirmasi.
- Fokus terlihat (outline ≥2px, kontras ≥3:1).
- Kontras teks ≥4,5:1, komponen UI ≥3:1, di kedua tema.
- Status tidak hanya dengan warna; perubahan status dan progres diumumkan lewat satu region `aria-live="polite"`; error lewat `role="alert"`.
- Target klik ≥24×24px.
- `@media (forced-colors: active)` memakai system colors; `prefers-reduced-motion` mematikan transisi.
- Teks UI dalam Bahasa Indonesia, dengan string terpusat di satu file agar bisa diterjemahkan.
- Bisa diperbesar sampai 200% (text scaling Windows) tanpa teks terpotong.

## 6. Penanganan error

| Kondisi | Perilaku |
|---|---|
| UAC dibatalkan | "Dibatalkan, tidak ada yang berubah." Rollback langkah non-admin. |
| Claude Desktop tidak terinstal / alias hilang | Error dengan tautan ke halaman unduh resmi; tombol Buka nonaktif. |
| Password ditolak | Status "Bermasalah", aksi "Perbaiki". |
| Junction hilang/rusak | Dideteksi saat buka; diperbaiki otomatis via bootstrap, lalu dilanjutkan. |
| RAM bebas < 2 GB | Konfirmasi "Tetap buka?" |
| `accounts.json` rusak | Dibackup ke `accounts.json.bad-<waktu>`, mulai kosong dengan akun Utama, beri tahu pengguna. |
| Instance kedua `claude-switcher` | Single-instance; peluncuran kedua membuka panel yang sudah ada. |

Log aplikasi: `%LOCALAPPDATA%\ClaudeSwitcher\logs\app.log` (rotasi 5×1 MB). Password dan token tidak pernah ditulis ke log.

## 7. Keamanan

- Password user tersembunyi hanya ada di Credential Manager (terenkripsi DPAPI untuk user utama) dan di memori sesaat saat dipakai; di-zeroize setelahnya.
- User tersembunyi adalah anggota grup Users (bukan Administrators) dan tidak tampil di layar login.
- Proses elevated hanya menerima operasi dari daftar tetap (`create_user`, `set_password`, `delete_user`) dengan validasi nama `^claude-[a-z0-9-]{1,13}$`; tidak pernah menjalankan perintah bebas.
- ACE yang diberikan hanya pada `<main>\.claude`, bukan seluruh profil.

## 8. Pengujian

- **Unit (cargo test):** `store` (round-trip, file rusak, tulis atomik), pembuatan slug/nama user, validasi argumen elevated, parsing log bootstrap, pemilihan proses di `monitor` dari daftar proses tiruan.
- **Integrasi (manual, Windows asli, butuh UAC):** skrip `tests/manual/README.md` — tambah akun sementara, buka bersamaan dengan Utama, cek junction & skills, tutup, perbaiki setelah password dirusak, hapus, dan pastikan tidak ada sisa user/ACE/kredensial.
- **UI:** panel dijalankan di browser dengan `invoke` tiruan; Playwright untuk alur keyboard dan `@axe-core/playwright` tanpa pelanggaran level A/AA, di tema terang, gelap, dan forced-colors. Pemeriksaan manual dengan Narrator.

## 9. Ketergantungan

Rust stable (rustup), Tauri v2 (`tauri`, `tauri-plugin-single-instance`, `tauri-plugin-autostart`), `windows` crate, `serde`/`serde_json`, `rand`, `zeroize`, `sysinfo`. Node.js (sudah ada) hanya untuk tes UI. WebView2 Runtime (sudah ada di Windows 11).
