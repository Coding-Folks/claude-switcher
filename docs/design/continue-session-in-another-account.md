# Lanjutkan sesi di akun lain

Status: desain, menunggu persetujuan · 2026-11-20

## Maksud

Saat satu akun Claude **kena batas pemakaian** di tengah sesi Claude Code, pengguna ingin
melanjutkan **sesi yang sama** di akun lain tanpa menyusun ulang konteks lewat prompt. Akun pertama
biasanya tidak dipakai lagi sampai batasnya pulih.

## Yang sudah ada

- Transkrip sesi ada di `~/.claude` (bersama untuk semua akun, lewat junction pada mode user Windows
  terpisah). Yang per akun hanya **daftar sesi**: `local_<id>.json` di
  `<data Claude>\claude-code-sessions\<akun>\<organisasi>\`, berisi `title`, `cwd`, `model`,
  `lastActivityAt`, `isArchived`, dan `cliSessionId` (penunjuk ke transkrip).
- "Impor riwayat" menyalin sesi untuk akun Claude yang **sama** dari folder Utama ke slot, dan meminta
  akun ditutup dulu. Belum ada pemindahan ke akun Claude yang **berbeda**.

## Tujuan

- Di menu ⋯ akun: **Lanjutkan sesi di akun lain…** → dialog: daftar sesi terbaru akun itu, akun
  tujuan, **Lanjutkan**.
- Sesi muncul di daftar Code akun tujuan, di paling atas, siap dilanjutkan dengan percakapannya utuh.
- Satu klik untuk kasus paling umum (sesi paling baru, ke akun lain yang tersedia).

## Bukan tujuan (v1)

- Sinkron otomatis antar semua akun (pendekatan B) dan pemindahan per folder proyek (C).
- Akun **User Windows terpisah** sebagai sumber atau tujuan (datanya di profil user lain, butuh UAC);
  di dialog tidak ditawarkan. Bisa menyusul lewat helper admin yang sudah ada.
- Memindahkan (menghapus dari akun asal): ini **menyalin**; sesi tetap ada di akun asal.

## Anti-tujuan

- Merusak daftar sesi akun tujuan (menimpa sesi yang lebih baru di sana, atau menulis saat Claude
  akun itu sedang menyimpan daftarnya sendiri).
- Diam-diam membuka sesi yang sama di dua akun sekaligus.

## Pendekatan yang ditimbang

| | Cara | Dipilih |
|---|---|---|
| A | Sekali klik per sesi dari menu ⋯, ke akun yang dipilih | **Ya** |
| B | Sinkron otomatis semua sesi ke semua akun | Tidak: daftar penuh di setiap akun, risiko tertimpa saat Claude terbuka, risiko dua akun di satu transkrip |
| C | Semua sesi satu folder proyek sekaligus | Tidak (bisa menyusul) |

Pengguna memilih A.

## Desain

**Akun yang ikut.** Utama dan akun *User Windows ini* (slot). Data Claude-nya:
Utama → `sessions_dirs(real LOCALAPPDATA)` (yang sudah ada: `Claude-Data\claude_*` dan
`Packages\Claude_*\LocalCache\Roaming\Claude`); slot → `Claude-Data\claude_*` di dalam slot.

**Daftar sesi sumber.** Semua `local_*.json` di bawah `claude-code-sessions\*\*` akun sumber,
`isArchived` ≠ true, diurutkan `lastActivityAt` menurun, 8 teratas. Baris: judul (`title`, atau
"Sesi tanpa judul"), nama folder terakhir dari `cwd`, waktu relatif. Yang paling baru terpilih.
Berkas yang tidak bisa dibaca sebagai JSON dilewati.

**Folder tujuan.** Di akun tujuan, folder `claude-code-sessions\<akun>\<organisasi>` yang **paling
baru diubah** (itulah akun yang sedang masuk). Tidak ada → galat "Masuk dulu ke akun <nama> di
Claude, lalu coba lagi" (sama seperti Impor riwayat).

**Menyalin.** Berkas ditulis dengan nama yang sama (`local_<id>.json`) ke folder tujuan:
- `lastFocusedAt` = sekarang (ms) supaya di paling atas daftar; `isArchived` = false; isi lain tetap.
- Sudah ada di tujuan dengan `lastActivityAt` **lebih baru** → tidak ditimpa, galat "Sesi ini lebih
  baru di <akun tujuan>". Lebih lama atau sama → ditimpa.
- Tulis ke berkas sementara lalu rename (atomik).

**Akun tujuan yang sedang terbuka.** Claude memuat daftar sesinya saat mulai dan menyimpannya sendiri,
jadi akun tujuan ditutup dulu (seperti Impor riwayat), berkas ditulis, lalu akun tujuan **dibuka**.
Dialog memberi tahu ini sebelum **Lanjutkan** ("<akun> akan ditutup dan dibuka lagi"). Akun asal
tidak disentuh.

**Dialog** (pakai `<dialog>` yang ada; komponen ala landing page):
```
Lanjutkan sesi di akun lain
Sesi dari Utama
 ◉ Sesi terakhir wedding app      Coding Folks · 5 mnt lalu
 ○ Fix login Google               claude-switcher · 2 jam lalu
Ke akun   [ Bobogeng Sharing ▾ ]
Bobogeng Sharing akan ditutup dan dibuka lagi.
                         [Batal]  [Lanjutkan]
```
- Menu ⋯ hanya menampilkan pilihan ini bila ada ≥ 2 akun yang ikut dan akun ini punya sesi.
- Pilihan sesi: radio (keyboard panah), daftar bergulir bila panjang. Akun tujuan: picker gaya
  landing page (seperti Bahasa); default akun ikut pertama selain sumber.
- Selesai: pengumuman "Sesi “…” dilanjutkan di <akun>." dan akun tujuan terbuka.

**Rust.** Modul `sessions.rs`: `list(dirs) -> Vec<SessionSummary>`, `target_dir(dirs) -> Option<PathBuf>`,
`plan_copy(src_json, dest_dir, now) -> Copy | Newer`, `write(...)` (fungsi murni + I/O tipis).
`Flows::continue_in(session_id, from, to)`; perintah Tauri `list_sessions(id)` dan
`continue_session(sessionId, from, to)`.

## Diserahkan ke pembuat

- Format waktu relatif (pakai pola teks yang ada di `strings.js`).
- Batas 8 sesi bisa diubah bila dialog terasa sempit.

## Verifikasi

- Rust: daftar terurut + arsip dilewati + JSON rusak dilewati; pilih folder tujuan terbaru; tidak
  menimpa yang lebih baru; `lastFocusedAt`/`isArchived` diperbarui; tanpa folder tujuan → galat.
- UI (mock): menu muncul/tidak, dialog keyboard + axe, pilihan default, pesan selesai, galat tampil.
- Manual: sesi nyata dari Utama dilanjutkan di slot; percakapan lengkap terlihat dan bisa diteruskan.
