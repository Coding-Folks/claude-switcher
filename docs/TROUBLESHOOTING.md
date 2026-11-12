# Troubleshooting

The app log is at `%LOCALAPPDATA%\ClaudeSwitcher\logs\app.log`. It never contains passwords; attach the
relevant lines when you open an issue.

| What you see | Why | What to do |
|---|---|---|
| "Claude Desktop tidak ditemukan" | Claude Desktop (MSIX) is not installed for this user | Install it from [claude.ai/download](https://claude.ai/download) |
| The tray icon does not open with the keyboard | The tray library only reports mouse clicks | Press **Ctrl+Alt+K**, or right-click the icon → **Buka panel** |
| An account shows **Bermasalah** | Its sign-in was refused, a link broke, or Claude did not open within 30 s | Press **Perbaiki** on the row |
| "Versi Claude ini tidak lagi memakai folder akun ini" | A Claude update changed how it picks its data folder | Remove that account and add it again with **User Windows terpisah** |
| "Folder akun … dibuat ulang. Login lagi" | The account's folder was deleted outside the app | Sign in again in the Claude window that opened |
| "Claude di akun ini belum tertutup" | Claude did not quit within 10 s before a remove or import | Close that Claude window, then try again |
| "Dibatalkan. Tidak ada yang berubah." | The UAC prompt was cancelled | Try again and choose **Yes** |
| A new account's Code tab is empty | The session list is kept per account | **⋯ → Impor riwayat** after signing in |
| A project under `C:\Users\<you>\…` will not open in a separate-user account | That Windows user cannot read your profile | Keep such projects outside your user folder (for example `D:\Projects`), or use **User Windows ini** |
| Windows SmartScreen blocks the installer | The installer is not code-signed yet | **More info → Run anyway**, after checking the SHA-256 on the release page |
| Claude Switcher does not start with Windows | Autostart points at an older copy | Start it once from the Start menu; it repoints autostart to itself |

## Removing everything

1. In the panel, remove every account (**⋯ → Hapus**). This deletes hidden Windows users, slot folders,
   folder permissions and stored passwords.
2. Uninstall **Claude Switcher** from Windows Settings → Apps.
3. Optionally delete `%APPDATA%\ClaudeSwitcher` and `%LOCALAPPDATA%\ClaudeSwitcher`.

Your own Claude Desktop data and `~/.claude` are never deleted by the app.
