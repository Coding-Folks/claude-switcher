# Update dari GitHub, komponen ala landing page, daftar akun yang menggulir

Status: diimplementasikan · 2026-11-19

## Maksud

1. **Update.** Pengguna tahu ada versi baru tanpa membuka GitHub, dan memasangnya dengan satu klik.
   Switcher **memberi tahu di panel**; pengguna yang memutuskan kapan menekan **Perbarui**.
2. **Komponen.** Tombol, input, dropdown dan toggle di panel harus *benar-benar mirip* komponen landing
   page Coding Folks (`landing-page/src/components/ui`). Spasi dirapikan. Jalur progress bar RAM di
   mode terang harus terlihat.
3. **Banyak akun.** Panel tidak boleh terus memanjang ke atas saat akun bertambah.

## Tujuan

- Cek update saat Switcher mulai (setelah 10 dtk) lalu tiap 6 jam; baris pemberitahuan di panel;
  **Perbarui** mengunduh, memeriksa tanda tangan, memasang (satu UAC), membuka ulang Switcher.
- Tombol pil, input 44 px, dropdown Bahasa dengan popover sendiri, toggle "Jalan saat Windows mulai".
- Daftar akun menggulir di dalam; kolom cari bila akun > 8.
- WCAG 2.2 AA tetap: semua tes axe, keyboard dan zoom 200% yang ada tetap lolos.

## Bukan tujuan

- Memasang update tanpa klik pengguna, atau tanpa UAC (instalasi per-machine selalu butuh UAC).
- Kanal beta / memilih versi; catatan rilis lengkap di panel (cukup versi + tautan "Lihat perubahan").
- Mengubah urutan atau mengelompokkan akun.

## Anti-tujuan

- Update yang tidak diverifikasi tanda tangannya (pilihan update "C" ditolak di diskusi karena ini).
- Dropdown buatan sendiri yang lebih buruk aksesibilitasnya dari `<select>` bawaan.
- Panel yang tingginya bergantung pada jumlah akun tanpa batas.

## Pendekatan yang ditimbang

| Bagian | Pilihan | Dipilih |
|---|---|---|
| Update | **A** `tauri-plugin-updater` + `latest.json` di rilis GitHub, ditandatangani | **Ya** |
| | B Cek GitHub API, Perbarui membuka halaman rilis | Tidak: pengguna tetap unduh/pasang manual |
| | C Unduh dan jalankan installer sendiri | Tidak: tanpa verifikasi tanda tangan |
| Dropdown | **A** Listbox buatan sendiri, setara Select landing page | **Ya** |
| | B `<select>` bawaan diberi gaya | Tidak: daftar terbukanya tetap gaya Windows |
| Banyak akun | Daftar menggulir di dalam + cari bila > 8 | Satu-satunya cara yang masuk akal |

Pengguna memilih A untuk update dan A untuk dropdown.

## Desain

### 1. Update

**Kunci.** Pengguna membuat pasangan kunci sekali: `cargo tauri signer generate -w ~/.tauri/claude-switcher.key`.
Kunci privat + kata sandinya menjadi secret GitHub `TAURI_SIGNING_PRIVATE_KEY` dan
`TAURI_SIGNING_PRIVATE_KEY_PASSWORD` (dilakukan pengguna sendiri). Kunci publik disimpan di
`src-tauri/updater.pub` (boleh dipublikasikan). **Selama `updater.pub` kosong, fitur update mati**
(plugin tidak didaftarkan, panel tidak menampilkan apa pun), jadi aplikasi tetap bisa dibangun.

**Aplikasi.**
- `tauri-plugin-updater` 2 didaftarkan di `lib.rs` dengan pubkey dari `include_str!("../updater.pub")`
  bila tidak kosong; endpoint
  `https://github.com/Coding-Folks/claude-switcher/releases/latest/download/latest.json`; Windows
  `installMode: "passive"` (bilah progres installer, UAC sekali).
- Modul `update.rs`: `check()` → `Option<UpdateInfo { version, notes_url }>`, disimpan di state;
  loop cek (10 dtk, lalu 6 jam) memancarkan `status` agar panel menggambar ulang.
- Perintah Tauri: `get_update() -> Option<UpdateInfo>`, `install_update()` (unduh dengan event
  `update-progress` {persen}, pasang, lalu `app.restart()`).
- Kegagalan (offline, tanda tangan salah, UAC ditolak) → pesan di `#alert`, log; tidak pernah crash.

**Panel.** Baris di bawah header, hanya bila ada update:
`[•] Versi 0.3.0 tersedia · Lihat perubahan        [Perbarui]` (tombol pil kecil brand).
Saat mengunduh: "Mengunduh pembaruan… 45%" (`aria-live=polite`), tombol nonaktif.
Teks dua bahasa di `strings.js`.

**Rilis (`release.yml`).** Setelah installer ditandatangani SignPath (atau tanpa SignPath):
`cargo tauri signer sign` atas `*-setup.exe` → `.sig`; skrip kecil menulis `latest.json`
(`version`, `pub_date`, `notes` = URL rilis, `platforms.windows-x86_64 {signature, url}`), dan
keduanya diunggah ke rilis draft. Langkah ini dilewati bila secret kunci belum ada. Rilis draft tidak
terlihat oleh `releases/latest` sampai dipublikasikan, jadi pengguna hanya mendapat rilis yang sudah
dirilis.

### 2. Komponen (nilai dari landing page)

| Komponen | Landing page | Panel |
|---|---|---|
| Tombol | `rounded-full`; `sm` h-9 px-4 13px medium; `md` h-11 px-6 16px semibold; primary `bg-brand` → `brand-hover`; outline border gray-100, putih, hover gray-50 | Semua tombol pil. Baris akun & dialog: `sm` (36 px). "Tambah akun" & kirim form: `md` (44 px). Outline: tepi `--control` (gray-400, karena gray-100 hanya 1,5:1 dan gagal 3:1) |
| Input | h-11, `rounded-2xl`, px-4, border, fokus: border brand + cincin 3px blue-50 | Sama; tepi `--control`; cincin `--ring` (blue-50 terang, `rgb(13 106 255 / .35)` gelap) |
| Select | Pemicu `rounded-2xl` + chevron yang berputar; popover `rounded-2xl` p-1.5, bayangan; item `rounded-xl` py-2 pl-3 pr-8 14px, sorot `brand/15`, terpilih teks brand + centang | Sama, versi terang & gelap. ARIA: tombol `aria-haspopup="listbox"` + `aria-expanded`, `ul[role=listbox]` dengan `aria-activedescendant`, `li[role=option][aria-selected]`. Keyboard: Enter/Spasi/↓/↑ membuka, ↑↓ Home End berpindah, ketik huruf melompat, Enter/Spasi memilih, Esc/Tab menutup, fokus kembali ke pemicu |
| Toggle | (tidak ada di landing page) | Turunan dari tombol: jalur pil 40×24, kenop 20 px putih; mati: jalur `--surface` + tepi `--control`; nyala: `--primary`. Tetap `input[type=checkbox][role=switch]` |
| Kartu | `rounded-3xl` border `surface-border` | Tetap 16 px (panel kecil), sesuai keputusan branding |

**Spasi.** Semua jarak pada kelipatan 4 px (6 px → 8 px, 10 px → 12 px, dst.); tepi 16, celah 12.

**Progress bar.** Jalur kosong memakai token baru `--track`: gray-100 `#D1D3D8` (terang),
black-700 `#161D27` → gray-900 `#2D3036` (gelap), bukan `--raised` yang sama dengan latar.
Legenda "Bebas" memakai `--track` yang sama.

### 3. Banyak akun

- `.accounts` mendapat `max-height` ≈ 6 baris (6 × 58 px) dan `overflow-y: auto`; scrollbar tipis
  dengan jempol `--accent` (kit memakai oranye untuk scrollbar), jalur transparan.
- Header, RAM, Tambah akun dan footer tidak ikut menggulir; `fitWindow` tetap mengukur isi, yang kini
  terbatas.
- Lebih dari 8 akun: input "Cari akun" di atas daftar (label tersembunyi), menyaring berdasarkan nama,
  tanpa beda huruf besar/kecil; Esc mengosongkan; "Tidak ada akun bernama …" bila kosong.
- Navigasi panah antar baris tetap jalan dan menggulir baris ke pandangan.

## Diserahkan ke pembuat

- Bentuk persis ikon chevron/centang (salin path SVG dari Select landing page).
- Teks pemberitahuan update persisnya, dalam gaya string yang ada.
- Nama skrip penulis `latest.json` (PowerShell di workflow atau `.mjs`).

## Verifikasi

- `tests-ui`: tes baru untuk dropdown (keyboard, ARIA, axe dengan popover terbuka), toggle, daftar
  menggulir (12 akun mock: tinggi panel tetap, baris terakhir bisa dicapai keyboard), cari, baris
  update (mock `get_update`), dan semua tes lama.
- `cargo test`: `update.rs` (pubkey kosong → mati; pembentukan `latest.json` bila ditulis di Rust).
- Manual setelah kunci dibuat: rilis uji v0.x.y → Switcher lama menampilkan baris → Perbarui → UAC →
  versi baru terbuka.
