# Branding Coding Folks untuk Claude Switcher

Status: diimplementasikan · 2026-11-19

## Maksud

Claude Switcher adalah produk Coding Folks, tapi panel dan asetnya belum memakai brand kit perusahaan:
panel sengaja netral ala Windows (Segoe UI, tombol hitam), dan ikon memakai biru/oranye yang mirip
tapi tidak sama dengan kit. Pengguna ingin **dua-duanya**: orang mengenali produk Coding Folks dari
ikon, README dan halaman rilis, **dan** panel terasa Coding Folks saat dipakai sehari-hari.

Sumber brand kit: `landing-page/src/styles/main.css` (blok `@theme`, sumber kebenaran) dan
`landing-page/docs/design-system.md`. `coding-folks-design-system.html` adalah referensi lama, tidak dipakai.

## Tujuan

- Panel memakai token kit: warna, Instrument Sans, IBM Plex Mono untuk angka, radius kit.
- Ikon aplikasi, tanda di header panel, ikon tray dan installer memakai warna persis kit dan sudut
  squircle 30%.
- Social preview dan banner README mengikuti gaya `og-image.png` landing page.
- Screenshot di `docs/assets/` dibuat ulang dari panel baru.

## Bukan tujuan

- Mengganti nama produk atau konsep ikonnya (dua kartu akun + titik oranye tetap).
- Memakai logo "CF" Coding Folks sebagai ikon aplikasi (itu logo perusahaan, bukan produk).
- Animasi masuk ala landing page (panel tray harus langsung siap pakai).

## Anti-tujuan

- Turun dari WCAG 2.2 AA: semua tes axe yang ada (terang, gelap, forced-colors, menu/dialog terbuka,
  Inggris) harus tetap lolos.
- Font dimuat dari internet (CSP `default-src 'self'`; font dibundel lokal).
- Oranye `#F48200` sebagai warna teks atau satu-satunya penanda status (2,6:1 di atas putih).

## Pendekatan yang ditimbang

| | Cara | Dipilih |
|---|---|---|
| A | Kit penuh di panel (font, warna, radius) + semua aset brand diperbarui | **Ya** |
| B | Warna kit saja, font tetap Segoe UI | Tidak: panel hanya sedikit terasa Coding Folks |
| C | Brand hanya di header/footer, isi panel netral | Tidak: pemakaian sehari-hari tetap terasa Windows |

Pengguna memilih A.

## Desain

### Token panel (`ui/styles.css`, variabel yang sudah ada diganti nilainya)

Rasio kontras dihitung dengan rumus WCAG.

| Variabel | Terang | Gelap | Catatan |
|---|---|---|---|
| `--ground` | gray-50 `#F0F1F2` | black-900 `#0D1117` | latar panel (= `surface` kit) |
| `--surface` | `#FFFFFF` | black-800 `#11171E` | kartu daftar akun |
| `--raised` | gray-50 `#F0F1F2` | black-700 `#161D27` | hover, strip RAM kosong |
| `--border` / `--rule` | gray-100 `#D1D3D8` | gray-800 `#3B3F46` | garis dekoratif |
| `--control` | gray-400 `#898E99` (3,3:1) | gray-500 `#686E7C` (3,5:1) | tepi input/checkbox, wajib ≥3:1 |
| `--text` | black-500 `#1F2937` (13:1) | `#FFFFFF` | |
| `--muted` | gray-600 `#616874` (5,0:1) | gray-300 `#9CA1AA` (7,0:1) | gray-500 kit hanya 4,5:1 di atas `--ground`, terlalu tipis |
| `--primary` | blue-500 `#0D6AFF` | blue-500 `#0D6AFF` | tombol utama; teks putih 4,6:1 |
| `--primary-hover` (baru) | blue-600 `#0C60E8` | blue-400 `#3D88FF` | |
| `--on-primary` | `#FFFFFF` | `#FFFFFF` | |
| `--link` (baru) | blue-600 `#0C60E8` (≥4,8:1) | blue-300 `#5D9BFF` (6,5:1) | teks biru; blue-500 hanya 4,1:1 di atas `--ground` |
| `--data` | blue-500 `#0D6AFF` | blue-300 `#5D9BFF` | RAM Claude (non-teks) |
| `--accent` (baru) | orange-500 `#F48200` | orange-400 `#F69B33` | hanya dekorasi: titik di tanda, penanda akun aktif bersama teks |
| `--negative`, `--positive` | tetap | tetap | kit tidak punya merah/hijau |
| `--focus` | black-500 `#1F2937` | `#FFFFFF` | |
| `--radius-card` | 16px (`rounded-2xl` kit) | | |
| `--radius-control` | 12px | | turunan: tombol 36px dengan 16px terlihat seperti pil |

Tombol sekunder: latar `--surface`, tepi `--control`, teks `--text` (tetap). Forced-colors: tidak
berubah (sudah memakai warna sistem).

### Tipografi

- `ui/fonts/instrument-sans-latin-wght-normal.woff2` dan `ibm-plex-mono-latin-500-normal.woff2` (atau
  400), disalin dari `landing-page/public/fonts` (OFL; lisensi ikut disalin ke `ui/fonts/OFL.txt`).
- `--font`: `"Instrument Sans", "Segoe UI Variable Text", "Segoe UI", system-ui, sans-serif`.
- `--font-mono` (baru): `"IBM Plex Mono", ui-monospace, monospace`, dipakai kelas `.num` yang sudah
  ada (angka RAM, jumlah berjalan, ukuran per akun).
- Bobot: judul 652, tombol/label 600, isi 456 (sumbu variabel Instrument Sans, seperti kit).
- Ukuran tetap (16/14/12 px): skala kit dibuat untuk halaman web, panel tray 320 px tetap kecil.

### Tanda dan ikon

- `assets/brand/logo-mark.svg` digambar ulang: kartu belakang blue-800 `#073A8C`, kartu depan gradien
  blue-700 `#094BB5` → blue-400 `#3D88FF`, titik gradien orange-600 `#DE7600` → orange-400 `#F69B33`,
  garis putih tetap. Sudut kartu `rx` = 30% dari sisi (604 → 181), sesuai `--radius-app-icon` kit.
- Header panel (`ui/index.html`, SVG inline) memakai bentuk dan warna yang sama.
- `assets/brand/icon-1024.png` dirender ulang, lalu `src-tauri/icons/*` dibuat ulang dengan
  `tauri icon` (ikon exe, tray, installer).

### Aset pemasaran

- `social-preview.png` (1280×640): gaya og-image landing page: latar biru gradien, "Claude Switcher"
  putih Instrument Sans 652 dengan titik oranye, tagline, kotak IBM Plex Mono
  `github.com/Coding-Folks/claude-switcher`, tanda di kanan.
- `social-preview-dark.png`: tata letak sama di atas black-900.
- `tests-ui/scripts/render-brand.mjs` memuat kedua font dari `ui/fonts` (bukan file opsional).
- Screenshot `docs/assets/panel-*.png` dibuat ulang dengan `tests-ui/scripts/docs-shots.mjs`.
- Footer panel: baris kecil "by Coding Folks" (teks, `--muted`), di kedua bahasa.

## Diserahkan ke pembuat

- Tagline social preview persisnya (ambil dari README).
- Detail hover/active tombol sekunder dan menu ⋯ selama memakai token di atas.
- Subset font: pakai file landing page apa adanya (latin).

## Verifikasi

- `npx playwright test` di `tests-ui`: semua tes axe dan fungsi lolos.
- Screenshot terang dan gelap dilihat berdampingan dengan landing page.
- `cargo test` tetap hijau (tidak ada perubahan Rust selain ikon).
