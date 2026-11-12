# How Claude Switcher works

This page explains what the app does on your machine, so you can judge it before you run it.
The full design notes (in Bahasa Indonesia) are in [`docs/design/`](design/).

## Why it is needed

Claude Desktop for Windows keeps one data folder per Windows user and allows one running copy per
data folder. A second copy started by the same user simply brings the first window forward. So, out of
the box, one Windows user means one signed-in Claude account.

Things we tried that do **not** give more than one extra copy (Claude Desktop 5.1231, Nov 2026):

| Attempt | Result |
|---|---|
| `--user-data-dir` or a different `APPDATA` | Ignored; the second copy hands over to the first |
| `CLAUDE_USER_DATA_DIR` | Present in the code but removed from the environment in release builds |
| A different `LOCALAPPDATA` alone | One extra copy, which falls back to a shared old profile |
| Sandboxie-Plus | Claude Desktop (MSIX) does not start in the sandbox |

Two things do work, and they are the two modes of the app.

## Mode 1 — this Windows user ("User Windows ini")

Claude Desktop computes its data folder from `LOCALAPPDATA`:
`<LOCALAPPDATA>\Claude-Data\claude_<package family>`. If that folder **already exists** before Claude
starts with a different `LOCALAPPDATA`, Claude uses it as is (its log says
`[user-data-move] already_moved_package_emptied`) and takes its single-instance lock there. So each
prepared folder is one more account.

For every such account the app creates:

```
%LOCALAPPDATA%\ClaudeSwitcher\slots\<account id>\Local
  ├─ Claude-Data\claude_<family>\      ← that account's Claude data (created before the first start)
  └─ Microsoft\WindowsApps             ← junction to your real %LOCALAPPDATA%\Microsoft\WindowsApps
```

and starts Claude through its normal app alias with `LOCALAPPDATA` set to that `Local` folder.
The `WindowsApps` junction lets Claude find its own alias; without it Claude removes its
"Open with" entry for your user. No admin rights are needed.

Removing such an account deletes that slot folder. The junction is unlinked, never followed, and the
app refuses any path that is not directly inside its own `slots` folder.

To tell the instances apart (they all run as you), the app reads the `--user-data-dir=` that Claude's
child processes carry on their command line.

## Mode 2 — a separate Windows user ("User Windows terpisah")

1. The app generates a random 32-character password and stores it in Windows Credential Manager
   (`ClaudeSwitcher/<user>`).
2. Through **one UAC prompt**, its admin helper creates a local user `claude-<name>`, makes it a
   standard user and hides it from the sign-in screen.
3. It grants that user Modify rights on your `%USERPROFILE%\.claude`.
4. As that user (`CreateProcessWithLogonW`), it registers Claude Desktop for them and makes their
   `%USERPROFILE%\.claude` a junction to yours, so skills, plugins, memory and Claude Code history
   are shared.
5. It starts Claude as that user. You sign in to the account you want there.

Removing it deletes the Windows user, their profile folder (unlinking the junction first, so your own
`.claude` is never touched), the folder permission and the stored password.

### The admin helper

`claude-switcher.exe --elevated …` runs one operation and exits. It only accepts `create_user`,
`set_password`, `delete_user` and `import_sessions`, only for names matching `claude-[a-z0-9-]{1,13}`,
and only reads or writes `<uuid>.secret` / `<uuid>.result.json` files directly inside
`%LOCALAPPDATA%\ClaudeSwitcher\tmp`. Passwords travel in a file there, never on a command line.

## What is shared and what is not

| | Shared between accounts | Per account |
|---|---|---|
| Skills, plugins, `CLAUDE.md`, memory, Claude Code transcripts (`~/.claude`) | ✓ | |
| Claude Desktop sign-in, cookies, settings | | ✓ |
| The Code tab's session **list** | | ✓ (use **Impor riwayat** to copy it) |
| Windows settings (mode 1 only): `claude://` link, "Open with", Claude's own "Launch at login" | ✓ | |

## Files the app writes

| Path | What |
|---|---|
| `%APPDATA%\ClaudeSwitcher\accounts.json` | The account list (no passwords) |
| `%LOCALAPPDATA%\ClaudeSwitcher\logs\app.log` | Log, 5 × 1 MB, passwords redacted |
| `%LOCALAPPDATA%\ClaudeSwitcher\slots\` | Mode 1 accounts |
| `%LOCALAPPDATA%\ClaudeSwitcher\tmp\`, `bootstrap\` | Short-lived files for the admin helper and first start |
| Windows Credential Manager `ClaudeSwitcher/*` | Mode 2 passwords |
