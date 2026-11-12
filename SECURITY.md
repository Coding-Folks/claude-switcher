# Security policy

Claude Switcher creates Windows users, stores their passwords and runs a small admin helper through
UAC, so we take reports seriously.

## Reporting a vulnerability

Please **do not open a public issue**. Use GitHub's private reporting instead:
**Security → Report a vulnerability** on
[github.com/Coding-Folks/claude-switcher](https://github.com/Coding-Folks/claude-switcher/security/advisories/new).

Include the version, Windows version, what you did, what happened, and what an attacker gains. We aim
to answer within 7 days and to ship a fix or mitigation within 30 days for confirmed issues, and we
credit reporters who want it.

## Supported versions

Only the latest release gets security fixes.

## What we consider in scope

- The admin helper (`--elevated`) doing anything beyond its four operations, on other users, or on
  files outside its exchange folder.
- Passwords of hidden users leaving Windows Credential Manager (on disk, in logs, on a command line).
- Removing an account deleting or changing anything outside that account (your `~/.claude`, your
  Claude data, the Claude Desktop install).
- A standard user escalating privileges through the app.

## Known limitations (not vulnerabilities)

- The installer and binaries are **not code-signed** yet. Verify the SHA-256 published with each
  release, or build from source.
- Accounts in the **same-Windows-user** mode are separated by data folder only; any program you run
  can read them, just like your main Claude data.
- Hidden Windows users are standard local users. Anyone with admin rights on the machine can manage
  them, as with any local account.
