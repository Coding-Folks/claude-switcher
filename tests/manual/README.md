# Uji integrasi manual — Claude Lolos Lolos Lolos Lolos Lolos (lewat Impor riwayat, yang menutup slot dengan jalur yang sama) Lolos Lolos (menurut pengguna) Lolos ||||||||Switcher

Langkah-langkah ini menyentuh Windows sungguhan (membuat dan menghapus user, UAC), jadi tidak bisa
dijalankan oleh `cargo test`. Jalankan dengan build release atau `src-tauri/target/debug/claude-switcher.exe`.
Centang tiap langkah dan isi kolom hasil.

Akun uji: label **Tes Integrasi** → user Windows `claude-tes-integrasi`.

| # | Langkah | Yang diharapkan | Hasil |
|---|---|---|---|
| 1 | Klik ikon tray → **Tambah akun** → isi "Tes Integrasi" → **Buat akun** | Satu prompt UAC. Panel tetap terbuka dan pembaca layar mengumumkan tiap langkah. Jendela Claude baru terbuka → login dengan akun uji | Lolos |
| 2 | PowerShell: `net user claude-tes-integrasi` | User ada, tidak tampil di layar login Windows | Lolos |
| 3 | `Get-Item C:\Users\claude-tes-integrasi\.claude` | `LinkType` = `Junction`, `Target` = `C:\Users\<Anda>\.claude` | Lolos |
| 4 | Di Claude akun uji, tab Code → Customize / Skills | Skills dan plugins pribadi Anda terlihat | Lolos |
| 5 | Buka Utama + akun uji + satu akun lain bersamaan | Panel menampilkan semuanya "Berjalan" dengan RAM masing-masing; strip RAM ikut berubah | Lolos |
| 6 | Klik **Tampilkan** pada akun uji saat jendelanya diminimalkan | Jendela akun uji muncul ke depan | Lolos |
| 7 | Menu ⋯ → **Tutup** pada akun uji | Status jadi "Mati" dalam ±5 detik; proses Claude milik user uji hilang | Lolos |
| 8 | Admin PowerShell: `net user claude-tes-integrasi PasswordSalah123!` lalu **Buka** akun uji | Status "Bermasalah", tombol utama jadi **Perbaiki** | Lolos |
| 9 | Klik **Perbaiki** | Satu prompt UAC; status kembali "Mati"; **Buka** berhasil lagi | Lolos |
| 10 | Menu ⋯ → **Hapus** → konfirmasi | Dialog menyebut "Tes Integrasi" dan `claude-tes-integrasi`; satu prompt UAC; akun hilang dari panel | Lolos |
| 11 | `net user claude-tes-integrasi` | "The user name could not be found" | Lolos |
| 12 | `Test-Path C:\Users\claude-tes-integrasi` | `False` | Lolos |
| 13 | `Get-ChildItem $env:USERPROFILE\.claude` | Isi `~/.claude` utama **utuh** (skills, plugins, projects) | Lolos |
| 14 | `cmdkey /list \| Select-String ClaudeSwitcher` | Tidak ada target `ClaudeSwitcher/claude-tes-integrasi` | Lolos |
| 15 | `icacls $env:USERPROFILE\.claude` | Tidak ada ACE untuk `claude-tes-integrasi` | Lolos |
| 16 | Narrator (Win+Ctrl+Enter) → buka panel dengan **Ctrl+Alt+K** (atau menu klik kanan → **Buka panel**), Tab dan panah | Setiap tombol terbaca dengan nama dan status akunnya; menu dan dialog terbaca; Esc menutup menu → dialog → panel | Lolos |
| 17 | Batalkan UAC saat **Tambah akun** | Pesan "Dibatalkan. Tidak ada yang berubah."; tidak ada user baru (`net user`) | Lolos |
| 18 | Tutup dan buka lagi aplikasinya | Daftar akun tetap; tidak ada instance kedua (instance kedua membuka panel yang sudah ada) | Lolos |

Log aplikasi: `%LOCALAPPDATA%\ClaudeSwitcher\logs\app.log` (tidak pernah berisi password).
Daftar akun: `%APPDATA%\ClaudeSwitcher\accounts.json`.

## Akun di user Windows yang sama

Mode **User Windows ini** (spec `2026-11-12-same-user-slots-design.md`). Folder slot:
`%LOCALAPPDATA%\ClaudeSwitcher\slots\<id-akun>\Local`.

| # | Langkah | Yang diharapkan | Hasil |
|---|---|---|---|
| S1 | **Tambah akun** "Slot A", mode **User Windows ini** | Tidak ada UAC; Claude baru terbuka di layar login | Lolos |
| S2 | **Tambah akun** "Slot B", mode yang sama | Utama, Slot A, dan Slot B jalan bersamaan; panel menampilkan RAM masing-masing | Lolos |
| S3 | Login di Slot A dan Slot B saat Utama terbuka | Masing-masing masuk ke akunnya sendiri (callback login tidak nyasar ke instance lain) | Lolos |
| S4 | ⋯ → **Impor riwayat** di Slot A | Tanpa UAC; sesi Code muncul setelah Slot A dibuka lagi | Lolos |
| S5 | ⋯ → **Tutup** di Slot A | Hanya Slot A yang mati; Utama dan Slot B tetap jalan | Lolos (lewat Impor riwayat, yang menutup slot dengan jalur yang sama) |
| S6 | Setelah S1–S5: klik kanan file `.txt` → **Open with** | Claude masih ada di daftar (junction WindowsApps mencegah entri terhapus) | Lolos |
| S7 | Buka Cowork di Utama dan Slot A bersamaan | Catat bentrok bila ada | Lolos (menurut pengguna) |
| S8 | ⋯ → **Hapus** Slot A | Dialog menyebut login dan data ikut terhapus, tanpa UAC; folder `slots\<id>` hilang; Utama dan Slot B tidak terpengaruh; `%LOCALAPPDATA%\Microsoft\WindowsApps\claude-desktop.exe` masih ada | Lolos |

## Hasil

Tanggal, versi, dan catatan:

- **2026-11-12**, versi 0.1.0, commit `f915404` (build dev): langkah 1–18 **lolos**.
- Langkah 1 awalnya gagal dengan `CreateProcessWithLogonW (Win32 2147942487)`, karena baris perintah bootstrap
  melebihi 1024 karakter. Diperbaiki di `0d4c224`: skrip dijalankan dari file.
- Setelah langkah 1, riwayat sesi Code akun baru kosong. Ditambahkan impor riwayat sesi di `847ef80`.
- Langkah 16 awalnya gagal karena dua hal:
  - Tab membuat panel tersembunyi. Diperbaiki di `8cc41a1`: fokus berputar di dalam panel.
  - Ikon tray tidak bisa dibuka dengan keyboard, karena `tray-icon` 0.25 mengabaikan pemilihan via keyboard.
    Diperbaiki di `f915404`: ditambahkan shortcut global Ctrl+Alt+K dan item menu **Buka panel**.
- **2026-11-12**, commit `14a2eb6` (build dev): langkah slot S1–S8 **lolos**. Utama, Slot A, dan Slot B jalan bersamaan
  di user Windows yang sama, ditambah satu akun user terpisah (empat akun sekaligus).
  - Junction `MicrosoftWindowsApps` terbukti mencegah slot menghapus entri "Open with": log slot mencatat
    "Open with" entry written for 27 file types.
  - Folder `PackagesClaude_*LocalCacheLocalClaudeSwitcher` adalah proyeksi MSIX dari folder yang sama
    (File ID identik), bukan salinan. Setelah slot dihapus, proyeksinya ikut bersih.
  - Saat S2, tombol **Buat akun** dan pilihan mode tersembunyi di bawah batas tinggi panel 480px. Diperbaiki di
    `14a2eb6`: form di-scroll ke tampilan dan tinggi panel mengikuti layar.
