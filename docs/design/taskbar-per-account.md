# Tombol taskbar per akun, dengan ikon Claude berwarna

Status: diimplementasikan · 2026-11-19

## Maksud

Semua jendela Claude Desktop (Utama dan setiap slot) membawa AppUserModelID (AUMID) paket MSIX yang
sama, jadi Windows menumpuknya di satu tombol taskbar oranye. Pengguna **susah membedakan jendela akun
yang sedang terbuka**. Setelah fitur ini, setiap akun punya tombol taskbar sendiri dengan ikon Claude
berwarna khas akun itu dan namanya, mis.

```
[🟠 Claude]  [🔵 Claude – Kerja]  [🟢 Claude – Klien A]
```

Tombol itu bisa di-pin (klik kanan → *Pin to taskbar*), dan pin itu membuka akun yang benar.

## Tujuan

- Jendela tiap akun non-Utama: tombol taskbar terpisah, ikon berwarna, nama "Claude – <label>".
- Ikon berwarna juga di Alt+Tab dan thumbnail.
- Pin taskbar / shortcut Start Menu per akun yang membuka akun itu.
- Bekerja untuk akun *User Windows ini* (slot). Akun *User Windows terpisah*: dicoba juga, best effort.

## Bukan tujuan (v1)

- Memilih warna sendiri di panel (warna diberikan otomatis; bisa menyusul).
- Ikon tray Claude per akun (tray dimiliki Claude, bukan kita).
- Menyematkan ke taskbar secara otomatis (Windows tidak mengizinkannya).

## Anti-tujuan

- Akun Utama berubah: AUMID-nya tetap bawaan, supaya pin Claude yang sudah ada tidak rusak.
- Mengubah file Claude atau paket MSIX-nya.
- Ikon kosong ("blank page") di taskbar, atau ikon yang hilang setelah Claude membuat ulang jendela.

## Pendekatan yang ditimbang

| | Cara | Hasil | Dipilih |
|---|---|---|---|
| A | AUMID + properti relaunch per jendela, shortcut Start Menu ber-AUMID sama, ikon berwarna | Tombol terpisah, warna terlihat di taskbar | **Ya** |
| B | Ikon jendela berwarna saja (WM_SETICON) | Tetap satu tumpukan; warna hanya di Alt+Tab/thumbnail | Tidak: tidak menjawab masalahnya |
| C | Nama akun di judul jendela | Tidak terlihat di tombol gabungan; Claude bisa menimpa judul | Tidak (bisa jadi tambahan nanti) |

Pengguna memilih A.

## Pelajaran dari proyek serupa (ide saja, kode tidak disalin)

Dari [multiclaude-windows](https://github.com/tcollins590/multiclaude-windows) (Apache-2.0),
[multi-claude](https://github.com/thomasog/multi-claude) (MIT) dan
[multi-claude-desktop](https://github.com/kenyoshizoe/multi-claude-desktop):

1. **Ikon dan nama tombol taskbar diambil dari shortcut Start Menu yang ber-AUMID sama**, bukan dari
   jendela. Ikon jendela hanya terpakai di Alt+Tab dan thumbnail. Jadi perlu keduanya.
2. **Claude kehilangan AUMID/ikon saat membuat ulang jendela** → pasang ulang secara berkala (±2–3 dtk).
3. **Taskbar Windows 11 memuat ikon dari proses ber-sandbox yang tidak bisa membaca folder user** →
   simpan `.ico` di `%PUBLIC%\ClaudeSwitcher\icons\`, bukan di `%LOCALAPPDATA%`.
4. **Explorer meng-cache ikon per path** → nama file memuat hash warna+versi, mis. `<id>-<hash>.ico`.
5. Properti relaunch harus diset **sebelum** `PKEY_AppUserModel_ID` (urutan dari dokumentasi Microsoft).

## Desain

**Warna.** Field baru `color` di `Account` (`#RRGGBB`, opsional untuk akun lama). Utama: tidak ada
(ikon asli). Akun baru dan akun lama yang belum punya: warna pertama dari palet tetap yang belum
dipakai (8 warna, kontras cukup di taskbar terang dan gelap), disimpan sekali.

**Ikon.** Diturunkan dari logo Claude yang terpasang di PC pengguna (PNG di folder `Assets` paket
MSIX): piksel oranye diwarnai ulang ke warna akun (pertahankan alpha dan kecerahan), lalu ditulis
sebagai `.ico` multi-ukuran (16–256 px, frame PNG). Dibuat saat akun dibuat/diperbaiki atau bila
file hilang; tidak didistribusikan bersama aplikasi.

**AUMID akun.** `CodingFolks.ClaudeSwitcher.Account.<id>` (stabil, tidak berubah saat ganti nama).

**Shortcut Start Menu.** `%APPDATA%\Microsoft\Windows\Start Menu\Programs\Claude Switcher\Claude – <label>.lnk`:
target `claude-switcher.exe --launch <id>`, ikon `.ico` akun, properti `PKEY_AppUserModel_ID` = AUMID
akun. Dibuat/diperbarui saat tambah, ganti nama, perbaiki; dihapus saat akun dihapus.

**Penandaan jendela.** Loop di Switcher (2 dtk): `EnumWindows` → jendela top-level terlihat yang
proses pemiliknya `root_pid` instance akun (dari `monitor::snapshot` + pemetaan instance→akun yang
sudah ada di `flows`). Untuk tiap jendela yang AUMID-nya belum benar:
`SHGetPropertyStoreForWindow` → set `RelaunchCommand` (`"claude-switcher.exe" --launch <id>`),
`RelaunchDisplayNameResource` ("Claude – <label>"), `RelaunchIconResource` (path `.ico`), lalu
`AppUserModel_ID`; plus `WM_SETICON` (kecil & besar) dengan ikon akun. Yang sudah benar dilewati.

**`--launch <id>`.** Diteruskan ke Switcher yang sedang jalan lewat plugin single-instance (argumen
kedua sudah sampai ke callback), atau ditangani di `setup` bila Switcher baru dimulai; memanggil
`flows.open(id)` seperti tombol **Buka**.

**Ganti nama / hapus akun.** Shortcut diganti nama/dihapus; jendela yang terbuka ditandai ulang pada
tick berikutnya. Hapus: file `.ico` ikut dihapus.

## Batasan yang diketahui

- Jeda ±2 dtk setelah jendela muncul sebelum pindah ke tombol sendiri.
- Akun *User Windows terpisah*: jendelanya milik user lain; bila Windows menolak menulis propertinya,
  akun itu tetap di tumpukan Claude (dicatat di log, tidak dianggap error).
- Update Claude bisa mengubah cara jendela dibuat; loop penandaan membuat ini pulih sendiri bila
  jendelanya masih top-level milik proses utama.

## Diserahkan ke pembuat

- Palet 8 warna persisnya, dan cara pewarnaan ulang (hue-shift vs tint) asalkan logo tetap terbaca.
- Crate untuk dekode PNG/tulis ICO (atau tulis ICO manual dengan frame PNG).
- Uji: pemilihan warna, nama/hash file ikon, pembentukan perintah relaunch, pemetaan jendela→akun
  (fungsi murni); verifikasi manual di taskbar Windows 11.
