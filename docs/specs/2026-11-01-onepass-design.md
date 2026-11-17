# one-pass design

Date: 2026-11-01

## Goal

A cross-platform (Linux, macOS, Windows) desktop vault for API keys and passwords that replaces plaintext secrets in `.env` files during local development. `.env` stays, but secret values are opaque references (`op://...`). A CLI (`op run -- <cmd>`) resolves them through the desktop app and launches the command, so an AI agent reading the project directory never sees a secret.

## Decisions

- Protection level: no secrets on disk or in the repo, unlock plus approval prompt (the `op run` model).
- Key custody: the desktop app is the only process that decrypts. The CLI talks to it over a local socket.
- Approval: per-project, time-boxed by default (15 min preselected). Per-run and remember-until-revoked are selectable per project.
- Unlock: master password only (Argon2id). File format leaves room for OS quick unlock and a secret key later.
- Data model: key-value secrets grouped by project and environment. Each entry has a `type` field defaulting to `secret` so logins and notes can be added without migration.
- Local only. No sync.
- Literal values in `.env` pass through. The CLI warns on values that look like secrets.

## Non-goals (v1)

Sync, import/export, browser autofill, OS quick unlock, secret key, typed entry UI beyond `secret`.

Known limit: a process running as the same OS user can read the child's environment while it runs, and a legitimate command can print its own environment. This design removes the "agent reads `.env`" path, not a fully hostile same-user process. The README must say so.

## Structure

Cargo workspace:

- `onepass-core`: crypto, vault format, data model, IPC protocol types. Fully testable without UI.
- `src-tauri`: desktop app. Owns the key, the IPC server, and Tauri commands. Depends on core.
- `onepass-cli`: standalone binary. Depends only on the protocol types, never on crypto or vault code, so a compromised CLI cannot decrypt anything.

Frontend: Angular standalone components and signals in `src/`, a thin client over Tauri commands.

## Vault

- File `vault.onepass` in the OS app-data directory.
- Versioned header: Argon2id salt and parameters.
- A random data key encrypts the body with XChaCha20-Poly1305. The password derives a key-encryption key that wraps the data key, so password changes only re-wrap.
- Writes are atomic: temp file, fsync, rename.
- Decrypted data exists only in app memory in zeroizing buffers.
- Tampering fails authentication and surfaces as a typed error.

## References and `.env`

- Reference format: `op://<project>/<environment>/<entry>`. Any other value starting with `op://` is a hard error.
- The CLI loads `.env` by default. `--env-file <path>` is repeatable, later files override earlier. `.env.local` is never loaded implicitly.
- Dotenv syntax (quotes, comments, `export`). No interpolation.
- Literals pass through unchanged. A stderr warning (variable name only, never the value) fires on key-like names (`*_KEY`, `*_TOKEN`, `*_SECRET`, `*_PASSWORD`) with a literal value, or on high-entropy literals. Warnings never block.
- The child inherits the parent environment with `.env` values layered on top. Exit codes and signals are forwarded. Resolved values are never written to disk or printed.

## IPC and approval

- Transport: Unix socket with `0600` on Linux/macOS, current-user-only named pipe on Windows.
- Request: `{ version, project_dir, command (argv), references: [op://...] }`. No secrets in requests.
- The app reads peer PID and executable (`SO_PEERCRED`, `LOCAL_PEERPID`, pipe client PID) and shows them in the dialog. This is informational: `command` and `project_dir` are client-claimed, so approvals are keyed on what the user sees and approves.
- Grant key: canonical `project_dir`, hash of the sorted reference set, executable name of the command. Any change prompts again.
- Modes per project: per-run (no grant stored), time-boxed (15 min, 1 h, or until lock; default), remember (grant hash persisted inside the encrypted vault, still invalidated by key changes).
- Time-boxed grants live in memory and die on lock or quit.
- Dialog shows project directory, command, caller PID and executable, variable names and entry names. Never values. Allow includes a duration selector in time-boxed mode.
- Locked vault: the app raises its window and asks for the master password. The CLI waits up to 60 s.
- Response on Allow: `{reference: value}` for the requested references only, then the connection closes. Deny, locked, missing entry, and version mismatch return typed errors; the CLI exits non-zero without launching anything.
- Hardening: cap request size and pending prompts; after 3 denials from one project within a minute, reject without prompting.
- Audit log: metadata only (project, entry names, outcome, time).

## Desktop UI

- Unlock screen and first-run vault creation.
- Browser: projects/environments sidebar, entry list, detail pane. Values masked until Reveal and re-hidden after 10 s. Copy clears the clipboard after 30 s.
- Entry editor: name, value, notes, Copy Reference button (`op://...`).
- Approvals: the dialog, a list of active grants with revoke, per-project mode settings.
- Audit log view.
- Settings: auto-lock timeout, change master password.
- Frontend gets entry metadata in bulk; values only through a single-entry reveal command.
- Tighten `tauri.conf.json`: real CSP, and fix `pnpm` vs `npm` in the build commands. Capabilities stay minimal.

## Errors

Typed errors across every boundary: wrong password, vault corrupt, vault locked, request denied, entry not found, protocol version mismatch. The CLI prints a short stderr message and exits non-zero without starting the child.

## Testing

- Core: encryption round-trip, wrong password, tamper detection, password change, format property tests.
- CLI: `.env` parsing edge cases, secret heuristics, reference parser.
- IPC: integration tests over a real socket with a fake approver: allow, deny, locked, rate limit, changed reference set.
- Manual: UI and Windows named pipe / macOS socket behavior on each OS.
- Run the `security-reviewer` agent after crypto, IPC, and capability changes.
