# Pemakaian per akun, layar Pengaturan, update sekarang, kontribusi

Status: diimplementasikan · 2026-11-20

## Maksud

Pengguna ingin **memilih akun yang masih aman dipakai sebelum kena limit**, langsung dari panel.
Sekalian: panel terasa sempit, perlu tempat untuk pengaturan, update yang bisa dipaksa tanpa
menunggu jadwal, tempat permanen untuk kartu "link login Google" (yang hilang setelah **Nanti**),
dan arahan bagi pengguna yang ingin berkontribusi.

## Yang sudah ada / yang bisa dibaca

- Transkrip Claude Code di `~/.claude/projects/*/<cliSessionId>.jsonl` (bersama semua akun): tiap
  pesan asisten membawa `timestamp` dan `message.usage` (`input_tokens`,
  `cache_creation_input_tokens`, `cache_read_input_tokens`, `output_tokens`). Error API tercatat
  dengan `isApiErrorMessage: true` dan `error` (contoh nyata: `authentication_failed`, "Login expired").
- Sesi milik sebuah akun: `local_*.json` di `claude-code-sessions\…` akun itu (punya `cliSessionId`),
  dibaca oleh `sessions.rs`. Cowork: `local-agent-mode-sessions\…` per akun; transkripnya mungkin di
  dalam VM Cowork, belum tentu terbaca.
- Sisa kuota resmi hanya bisa diambil dari server dengan token login akun → **tidak dipakai**.

## Tujuan

1. **Pemakaian per akun** (Claude Code, + Cowork bila transkripnya terbaca): token dalam **5 jam
   terakhir** dan **7 hari terakhir**, dihitung dari transkrip sesi milik akun itu.
2. **Deteksi limit otomatis**: error API jenis batas pemakaian di transkrip akun itu → akun ditandai
   "kena limit" dengan jam reset bila pesannya menyebutkan jam (kalau tidak: +5 jam dari error).
3. **Tanda limit manual** sebagai cadangan: ⋯ → **Tandai kena limit…** (jam reset, default +5 jam);
   ⋯ → **Hapus tanda limit**.
4. **Batas yang dipelajari**: saat akun kena limit (otomatis atau manual), token 5 jam pada saat itu
   disimpan sebagai batas akun; selanjutnya ditampilkan "± N% dari batas". Batas baru = rata-rata
   bergerak dari 3 kejadian terakhir.
5. **Tampilan**: baris akun mendapat baris ketiga ringkas; urutan tetap (tidak diurutkan ulang),
   tetapi akun yang kena limit tampak redup dengan hitung mundur.
6. **Layar Pengaturan** (ikon roda gigi di header; tombol kembali): Jalan saat Windows mulai; Cek
   versi baru + **Cek sekarang** (+ **Perbarui sekarang** bila ada versi baru); Bahasa; **Link login
   Google** (status: "Claude Switcher" / "Claude — perlu diatur" + tombol pengaturan Windows);
   **Tentang & kontribusi** (versi, GitHub, Laporkan masalah, Cara berkontribusi, lisensi MIT,
   "by Coding Folks").
7. **Panel utama lebih lega**: footer pindah ke Pengaturan; yang tersisa: header (dengan roda gigi),
   kartu-kartu pemberitahuan, RAM, daftar akun, **Tambah akun**.

## Bukan tujuan

- Sisa kuota resmi dari Anthropic; pemakaian **Chat** (tersimpan di server).
- Mengurutkan ulang akun otomatis; notifikasi Windows saat limit pulih (bisa menyusul).
- Akun *User Windows terpisah*: transkripnya bersama (junction), tetapi daftar sesinya di profil user
  lain yang tak terbaca tanpa admin → pemakaiannya tidak ditampilkan (baris tetap tampil tanpa detail).

## Anti-tujuan

- Menyentuh token login atau mengirim data pemakaian ke mana pun (semua dihitung lokal).
- Angka yang terlihat pasti padahal perkiraan: selalu "±", dan teks bantuan menjelaskan dasarnya.
- Panel melambat: pembacaan transkrip tidak boleh memblokir UI.

## Pendekatan

| | Cara | Dipilih |
|---|---|---|
| A | Perkiraan dari transkrip + deteksi/tanda limit + batas yang dipelajari | **Ya** |
| B | Tanda limit manual saja | Tidak: tidak menunjukkan akun yang hampir habis |
| C | Sisa kuota resmi lewat token login | Tidak: menyentuh token login |

Pengguna memilih A, dan meminta kekurangannya diperbaiki sejauh mungkin (poin 1–4 di Tujuan).

## Desain

**Penghitungan (Rust, `usage.rs`).**
- Peta akun → `cliSessionId` dari `local_*.json` sesi akun (semua, bukan hanya 8 terbaru; termasuk
  `priorCliSessionIds` bila ada) dan dari sesi Cowork bila berkas serupa ditemukan.
- Untuk tiap `cliSessionId`: berkas `~/.claude/projects/*/<id>.jsonl` (+ `<id>/subagents/*.jsonl`).
  Baca hanya baris dengan `message.usage` dan `timestamp` dalam 7 hari; jumlah token =
  input + cache_creation + output (cache_read **tidak** dihitung penuh: ditimbang 0,1 karena jauh lebih
  murah dalam kuota; konstanta di satu tempat, dicatat sebagai perkiraan).
- **Inkremental**: simpan offset baca per berkas + ringkasan per jam (`usage-cache.json` di
  `%LOCALAPPDATA%\ClaudeSwitcher`), sehingga tiap putaran hanya membaca baris baru. Putaran tiap 60 dtk
  di thread latar; hasil di state, panel membaca lewat `list_accounts` (field baru di `AccountView`).
- **Deteksi limit**: baris dengan `isApiErrorMessage: true` dan (`error` mengandung `rate_limit` atau
  teks berisi "limit" + "reset"/"resets") dalam 5 jam terakhir → `limited_until` = jam reset hasil
  parse teks (format "resets 7pm", "resets at 19:00", epoch setelah `|`) atau +5 jam. Polanya belum
  terlihat nyata → parser toleran + tes dari contoh pesan yang diketahui; manual tetap ada.
- **Batas dipelajari**: saat status berubah ke kena limit, token 5 jam saat itu dicatat ke
  `accounts.json` (`limit_tokens`, rata-rata 3 terakhir).

**Data ke panel** (`AccountView` tambahan): `usage_5h`, `usage_7d`, `limited_until` (ms|null),
`limit_source` ("auto"|"manual"|null), `limit_tokens` (null bila belum dipelajari).

**Baris akun** (contoh):
```
Bobogeng Sharing        ○ Mati · User ini
  5 jam ▂▂▃░░░ 0,4 jt token · ± 30% dari batas        [Buka] ⋯
Utama                   ● Berjalan · 4,6 GB
  Kena limit · pulih 19:00 (2 j 14 m)                  [Tampilkan] ⋯
```
Bar membandingkan dengan batas yang dipelajari bila ada, kalau tidak dengan akun terbesar. Tooltip /
teks bantuan (`aria-describedby`): "Perkiraan dari sesi Claude Code (dan Cowork) di komputer ini;
Chat tidak terhitung."

**Pengaturan.** Tampilan kedua di jendela panel yang sama (bukan jendela baru): header
"← Pengaturan", lalu bagian-bagian di atas sebagai kartu. Fokus ke tombol kembali saat dibuka; Esc
kembali ke daftar. **Cek sekarang** → perintah `check_update_now` (memanggil cek yang sama, mengabaikan
jadwal 6 jam; hanya bila izin cek menyala, kalau mati tombolnya meminta izin dulu). **Perbarui
sekarang** = `install_update` yang ada. Link login Google: status dari `link_handler`; kartu di daftar
tetap muncul seperti sekarang, dan **Nanti** kini juga tersimpan di sini.

**Kontribusi.** Tautan dibuka lewat Rust (`open_url` hanya untuk daftar URL tetap: repo, issues/new
dengan template bug, CONTRIBUTING.md, lisensi). README dan CONTRIBUTING mendapat bagian singkat
"Cara membantu" (lapor bug, terjemahan, kode) yang sama dengan isi layar.

## Diserahkan ke pembuat

- Bobot cache_read (0,1) dan format angka token (rb/jt).
- Lokasi transkrip Cowork: hitung bila ditemukan, tanpa memperlambat; kalau tidak ditemukan, teks
  bantuan menyebut "Claude Code".
- Ukuran dan isi `usage-cache.json`.

## Verifikasi

- Rust: penjumlahan jendela 5 jam/7 hari dari baris contoh; baca inkremental; deteksi limit dari
  beberapa bentuk pesan; parse jam reset; batas dipelajari (rata-rata 3); akun tanpa sesi → 0.
- UI: baris pemakaian & kena limit (mock), tanda manual, layar Pengaturan (keyboard, Esc, axe),
  Cek sekarang/Perbarui sekarang, link login Google di Pengaturan, tautan kontribusi.
- Nyata: angka 5 jam salah satu akun masuk akal terhadap pemakaian hari itu.
