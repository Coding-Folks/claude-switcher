// All UI copy in one place (Bahasa Indonesia). {name} placeholders are filled by t().
window.STRINGS = {
  title: "Akun Claude",
  running_count: "{n} berjalan",
  none_running: "Belum ada yang berjalan",
  ram_finding: "RAM bebas {free} · cukup untuk ±{n} akun lagi",
  ram_finding_low: "RAM bebas {free} · tidak cukup untuk akun lagi",
  ram_strip_label: "Pemakaian RAM: Claude {claude}, lainnya {other}, bebas {free} dari {total}",
  ram_legend_claude: "Claude",
  ram_legend_other: "Lainnya",
  ram_legend_free: "Bebas",

  state_running: "Berjalan · {mem}",
  state_stopped: "Mati",
  state_opening: "Membuka…",
  state_problem: "Bermasalah",
  main_account_note: "user Windows Anda",

  open: "Buka",
  show: "Tampilkan",
  options_for: "Opsi untuk {label}",
  menu_close: "Tutup",
  menu_rename: "Ganti nama",
  menu_repair: "Perbaiki",
  menu_remove: "Hapus",

  add_account: "Tambah akun",
  add_label: "Nama akun",
  add_hint: "Misalnya Kantor atau Klien. Windows akan meminta persetujuan admin sekali.",
  add_submit: "Buat akun",
  cancel: "Batal",
  add_empty: "Isi nama akun dulu.",

  progress_vault: "Menyimpan kredensial…",
  progress_create_user: "Membuat user Windows… setujui prompt admin",
  progress_grant: "Menyambungkan skills dan plugins…",
  progress_bootstrap: "Menyiapkan Claude untuk akun ini…",
  progress_launch: "Membuka Claude…",
  add_done: "Akun {label} siap. Login di jendela Claude yang terbuka.",
  opened: "{label} dibuka.",
  focused: "{label} ditampilkan.",
  closed: "{label} ditutup.",
  renamed: "Nama diganti menjadi {label}.",
  repaired: "{label} sudah diperbaiki.",
  removed: "Akun {label} dihapus.",

  remove_title: "Hapus akun {label}?",
  remove_body: "User Windows {user} beserta data login Claude-nya akan dihapus permanen. Skills, plugins, dan memory bersama tidak ikut terhapus.",
  remove_confirm: "Hapus akun",
  lowram_title: "RAM bebas tinggal {free}",
  lowram_body: "Membuka {label} bisa membuat komputer lambat.",
  lowram_confirm: "Tetap buka",
  rename_title: "Ganti nama {label}",
  rename_submit: "Simpan",
  rename_label: "Nama baru",

  autostart: "Jalan saat Windows mulai",

  "err.uac_cancelled": "Dibatalkan. Tidak ada yang berubah.",
  "err.claude_missing": "Claude Desktop tidak ditemukan. Instal dulu dari claude.ai/download.",
  "err.logon_failed": "Windows menolak login akun ini. Pilih Perbaiki di menu opsinya.",
  "err.junction_broken": "Sambungan skills dan plugins rusak. Pilih Perbaiki.",
  "err.low_memory": "RAM tidak cukup untuk membuka akun lagi.",
  "err.store_corrupt": "Daftar akun rusak dan sudah dicadangkan ke {detail}. Daftar dimulai ulang; user Windows akun lama masih ada.",
  "err.not_found": "Akun tidak ditemukan.",
  "err.open_timeout": "Claude tidak terbuka dalam 30 detik.",
  "err.generic": "Terjadi kesalahan: {detail}",
};

window.t = function t(key, vars) {
  let s = window.STRINGS[key] ?? key;
  for (const [k, v] of Object.entries(vars || {})) s = s.replaceAll(`{${k}}`, String(v));
  return s;
};
