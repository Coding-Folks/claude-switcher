# Opsi "user Windows yang sama" saat menambah akun

Tanggal: 2026-11-12. Melengkapi `design.md`; bagian yang tidak disebut di sini tetap berlaku.

## 1. Tujuan

Saat **Tambah akun**, pengguna memilih tempat akun baru dijalankan:

- **User Windows ini** (baru): Claude jalan sebagai user utama dengan folder data sendiri. Tanpa UAC, tanpa user Windows baru, dan jumlah akunnya tidak dibatasi.
- **User Windows terpisah** (yang sudah ada): satu user Windows tersembunyi per akun, terisolasi penuh, dan perlu UAC.

## 2. Temuan (diuji 2026-11-12, Claude MSIX 5.1231.2.0)

Spec utama §2 menyimpulkan bahwa override `LOCALAPPDATA` hanya memberi 1 slot. Kesimpulan itu tidak berlaku lagi untuk versi ini.

- Claude menyimpan datanya di `<LOCALAPPDATA>\Claude-Data\claude_<id-family>`. Path ini dihitung dari variabel lingkungan `LOCALAPPDATA` (`$6()`/`v4()` di `app.asar`).
- Kunci single-instance dipasang di folder data yang dipilih (`requestInstanceLock` → `setPath("userData", …)`). Jadi folder data yang berbeda berarti kunci yang berbeda juga.
- Kalau `<X>\Claude-Data\claude_<id-family>` **sudah ada** sebelum Claude dijalankan dengan `LOCALAPPDATA=X`, Claude memakainya. Log mencatat `[user-data-move] already_moved_package_emptied`. Penanda migrasi hanya ditulis ke `X\Packages\…` (palsu), sehingga data Claude utama tidak tersentuh.
- Spec lama mendapat 1 slot karena folder itu tidak disiapkan. Akibatnya Claude kembali memakai profil lama `LocalCache\Roaming\Claude` yang dipakai bersama oleh semua override.
- Hasil uji: Claude utama dan dua slot (`slot1`, `slot2`) jalan bersamaan, masing-masing dengan `--user-data-dir` sendiri. Slot mulai kosong (sekitar 25 MB), dan login di slot berjalan normal.
- Login dengan akun Claude yang sama di slot menghasilkan daftar sesi Code yang kosong. Setelah `claude-code-sessions` dari data utama disalin ke slot (tanpa menimpa) dan slot dibuka ulang, sesi-sesinya muncul.
- `CLAUDE_USER_DATA_DIR` dan `--user-data-dir` tetap tidak bisa dipakai. Variabel itu dihapus dari env oleh build rilis (`U2`), kecuali dalam mode debug.

### Efek samping yang harus dicegah

Slot mencatat `osEntryPoints: "Open with" entry removed: no app execution alias at <X>\Microsoft\WindowsApps\Claude_…\claude-desktop.exe`. Artinya slot menghapus entri "Open with" milik user ini di HKCU, karena alias tidak ditemukan di `LOCALAPPDATA` palsu.

Pencegahan: sebelum slot dijalankan, buat junction `<X>\Microsoft\WindowsApps` → `%LOCALAPPDATA%\Microsoft\WindowsApps` yang asli. **Belum diuji**; uji manual langkah S6 memverifikasinya.

## 3. Model data

`AccountKind` mendapat varian `SameUser` (JSON `"same_user"`). `Account` mendapat field opsional `data_root: Option<PathBuf>` (`#[serde(default, skip_serializing_if = "Option::is_none")]`). Isinya `LOCALAPPDATA` palsu milik slot:

```
%LOCALAPPDATA%\ClaudeSwitcher\slots\<account-id>\Local
  ├─ Claude-Data\claude_<id-family>\   ← data Claude slot (dibuat app sebelum peluncuran pertama)
  └─ Microsoft\WindowsApps             ← junction ke WindowsApps asli
```

- Folder slot dinamai dengan `id` akun, bukan label, supaya rename tidak memindahkan data.
- `windows_user` tetap `None` untuk `SameUser`.
- `accounts.json` tetap `version: 1`. File lama tetap terbaca. Versi app lama akan menganggap file berisi `same_user` rusak, lalu membackupnya. Ini dapat diterima karena hanya dipakai pada satu mesin.

## 4. Alur

| Alur | `WindowsUser` (sekarang) | `SameUser` (baru) |
|---|---|---|
| Tambah | vault → UAC buat user → grant → bootstrap → simpan → luncurkan | buat folder slot + junction → simpan → luncurkan. Tanpa UAC dan vault. Rollback: hapus folder slot |
| Buka | `launch_as` (CreateProcessWithLogonW) | spawn alias asli dengan env `LOCALAPPDATA=<data_root>`. Kalau folder atau junction hilang, buat ulang dulu |
| Status/RAM | instance dengan `owner_sid` user itu | instance milik SID utama yang `--user-data-dir`-nya = `<data_root>\Claude-Data\claude_<family>` |
| Tutup | taskkill sebagai user itu | taskkill biasa pada root pid slot |
| Impor riwayat | elevated `ImportSessions` | tanpa elevasi: tutup slot → `history::import_plan(sessions_dirs(LOCALAPPDATA asli), <slot>\…\claude-code-sessions)` → `copy_plan` |
| Perbaiki | password/junction/bootstrap | buat ulang folder data dan junction WindowsApps |
| Hapus | tutup → revoke → UAC hapus user → vault | tutup → hapus folder slot. Dialog konfirmasi menyebut bahwa login dan data slot ikut terhapus |

### Membedakan instance di user yang sama

Saat ini `monitor` hanya mengelompokkan instance berdasarkan `owner_sid`. Untuk SID utama, aturannya diubah:

- Proses anak Claude membawa `--user-data-dir=<path>` di baris perintahnya. Proses milik user sendiri bisa dibaca baris perintahnya tanpa admin.
- Root instance adalah induk dari proses-proses anak dengan path yang sama.
- `InstanceStatus` mendapat field `data_dir: Option<PathBuf>`.
- Akun `Main` cocok dengan instance SID utama yang `data_dir`-nya **bukan** milik slot mana pun. Jadi Utama tetap terdeteksi meskipun lokasi datanya berpindah.
- Pembandingan path dilakukan tanpa membedakan huruf besar/kecil, setelah dinormalisasi.

## 5. UI

Form **Tambah akun** mendapat `fieldset` dengan `legend` "Jalankan di", berisi dua radio:

- **User Windows ini** (default). Petunjuk: "Cepat, tanpa izin admin. Berbagi pengaturan Windows dengan akun Utama."
- **User Windows terpisah**. Petunjuk: "Terisolasi penuh. Perlu izin admin (UAC) sekali."

Ketentuan lain:

- Teks progres disesuaikan, karena langkah UAC tidak ada untuk mode pertama.
- Di daftar akun, tiap baris menampilkan jenisnya sebagai teks sekunder ("User ini" / "User terpisah"), dan teks itu ikut dibaca lewat `aria-describedby`.
- Semua ketentuan aksesibilitas di spec utama §5 tetap berlaku. Tes Playwright/axe ditambah untuk radio group: navigasi dengan panah, label terbaca, dan default terpilih.

## 6. Error

| Situasi | Perilaku |
|---|---|
| Folder slot gagal dibuat (disk penuh, izin) | Error "Gagal menyiapkan folder akun." dan tidak ada yang disimpan |
| Claude tidak terinstal / alias hilang | Sama seperti spec utama (`ClaudeMissing`) |
| Slot tidak muncul dalam 30 detik | Status "Bermasalah" (sama seperti spec utama), aksi Perbaiki membuat ulang folder |
| Folder slot dihapus di luar app | Saat Buka, folder dibuat ulang (akun harus login lagi) dan pengguna diberi tahu lewat live region |

## 7. Risiko yang belum diuji

- **Pengaturan bersama di HKCU**: registrasi protokol `claude://`, autostart, dan "Open with" dipakai bersama semua slot. Kalau "Launch at login" dinyalakan dari dalam slot, yang terdaftar adalah alias tanpa env, sehingga yang terbuka saat login adalah Utama. Pengguna diarahkan memakai autostart Claude Switcher.
- **Login via browser** (`claude://` callback) saat beberapa slot terbuka: callback bisa jatuh ke instance yang salah. Login di slot2 berhasil, tapi caranya tidak tercatat. Uji S3 memverifikasinya.
- **Cowork / `cowork-svc`** dipakai per user Windows. Slot bersamaan mungkin bentrok. Uji S7 memverifikasinya.
- **Pembaruan Claude** bisa mengubah logika folder data. Bootstrap memeriksa log slot (`[user-data-move]`). Kalau labelnya bukan `already_moved_*`, slot ditandai Bermasalah dan app menyarankan mode user terpisah.

## 8. Uji

**Unit (`FakePlatform`):**
- `add_same_user` tidak memanggil `elevated`/`vault_*`; folder dibuat sebelum simpan; rollback menghapus folder bila simpan gagal.
- `open_same_user` memakai `launch_slot` dengan `data_root` yang benar.
- `remove_same_user` menutup lalu menghapus folder, tanpa UAC.
- `import_same_user` berjalan tanpa elevasi.
- Monitor: tiga instance dengan SID sama dan `--user-data-dir` berbeda menghasilkan tiga status. Utama tetap cocok saat ada slot.
- Store: file lama tanpa `data_root` tetap terbaca; `same_user` round-trip.

**Manual** (ditambahkan ke `tests/manual/README.md`, langkah S1–S8):
1. **S1:** Tambah "Slot A" dengan mode **User Windows ini**. Tidak ada UAC, dan Claude baru terbuka di layar login.
2. **S2:** Tambah "Slot B" dengan mode yang sama. Utama, Slot A, dan Slot B jalan bersamaan dengan RAM masing-masing.
3. **S3:** Login di Slot A dan Slot B saat Utama terbuka. Masing-masing masuk ke akunnya sendiri.
4. **S4:** Impor riwayat ke Slot A tanpa UAC. Sesi Code muncul setelah Slot A dibuka lagi.
5. **S5:** Tutup Slot A. Hanya Slot A yang mati.
6. **S6:** Setelah S1–S5, klik kanan file `.txt` → Open with. Claude masih ada di daftar. Ini memverifikasi junction WindowsApps.
7. **S7:** Buka Cowork di Utama dan Slot A bersamaan. Catat bentrok bila ada.
8. **S8:** Hapus Slot A. Folder `slots\<id>` hilang, sedangkan Utama dan Slot B tidak terpengaruh.
