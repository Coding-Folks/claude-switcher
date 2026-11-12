# Uji integrasi manual — Claude Switcher

Langkah-langkah ini menyentuh Windows sungguhan (membuat dan menghapus user, UAC), jadi tidak bisa
dijalankan oleh `cargo test`. Jalankan dengan build release atau `src-tauri/target/debug/claude-switcher.exe`.
Centang tiap langkah dan isi kolom hasil.

Akun uji: label **Tes Integrasi** → user Windows `claude-tes-integrasi`.

| # | Langkah | Yang diharapkan | Hasil |
|---|---|---|---|
| 1 | Klik ikon tray → **Tambah akun** → isi "Tes Integrasi" → **Buat akun** | Satu prompt UAC. Panel tetap terbuka dan pembaca layar mengumumkan tiap langkah. Jendela Claude baru terbuka → login dengan akun uji | |
| 2 | PowerShell: `net user claude-tes-integrasi` | User ada, tidak tampil di layar login Windows | |
| 3 | `Get-Item C:\Users\claude-tes-integrasi\.claude` | `LinkType` = `Junction`, `Target` = `C:\Users\<Anda>\.claude` | |
| 4 | Di Claude akun uji, tab Code → Customize / Skills | Skills dan plugins pribadi Anda terlihat | |
| 5 | Buka Utama + akun uji + satu akun lain bersamaan | Panel menampilkan semuanya "Berjalan" dengan RAM masing-masing; strip RAM ikut berubah | |
| 6 | Klik **Tampilkan** pada akun uji saat jendelanya diminimalkan | Jendela akun uji muncul ke depan | |
| 7 | Menu ⋯ → **Tutup** pada akun uji | Status jadi "Mati" dalam ±5 detik; proses Claude milik user uji hilang | |
| 8 | Admin PowerShell: `net user claude-tes-integrasi PasswordSalah123!` lalu **Buka** akun uji | Status "Bermasalah", tombol utama jadi **Perbaiki** | |
| 9 | Klik **Perbaiki** | Satu prompt UAC; status kembali "Mati"; **Buka** berhasil lagi | |
| 10 | Menu ⋯ → **Hapus** → konfirmasi | Dialog menyebut "Tes Integrasi" dan `claude-tes-integrasi`; satu prompt UAC; akun hilang dari panel | |
| 11 | `net user claude-tes-integrasi` | "The user name could not be found" | |
| 12 | `Test-Path C:\Users\claude-tes-integrasi` | `False` | |
| 13 | `Get-ChildItem $env:USERPROFILE\.claude` | Isi `~/.claude` utama **utuh** (skills, plugins, projects) | |
| 14 | `cmdkey /list \| Select-String ClaudeSwitcher` | Tidak ada target `ClaudeSwitcher/claude-tes-integrasi` | |
| 15 | `icacls $env:USERPROFILE\.claude` | Tidak ada ACE untuk `claude-tes-integrasi` | |
| 16 | Narrator (Win+Ctrl+Enter) → buka panel, Tab dan panah | Setiap tombol terbaca dengan nama dan status akunnya; menu dan dialog terbaca; Esc menutup menu → dialog → panel | |
| 17 | Batalkan UAC saat **Tambah akun** | Pesan "Dibatalkan. Tidak ada yang berubah."; tidak ada user baru (`net user`) | |
| 18 | Tutup dan buka lagi aplikasinya | Daftar akun tetap; tidak ada instance kedua (instance kedua membuka panel yang sudah ada) | |

Log aplikasi: `%LOCALAPPDATA%\ClaudeSwitcher\logs\app.log` (tidak pernah berisi password).
Daftar akun: `%APPDATA%\ClaudeSwitcher\accounts.json`.

## Hasil

Tanggal, versi, dan catatan:
