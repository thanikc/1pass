# Plan: app improvements

Date: 2026-11-16. Branch `feat/app-improvements`. Specs in `docs/specs/`, scenarios in `docs/features/`.

Each step is test-first: write the failing test for its scenarios, see it fail, make it pass. Steps run in order; each ends green before the next starts.

## Decisions to confirm

1. **Import adds one dependency**, `tauri-plugin-dialog`, for the native file picker. It is called from Rust only, so `capabilities/default.json` does not change.
2. **Import never edits your `.env`.** It shows the `NAME=op://...` lines to paste. An existing entry is overwritten only when you tick it.
3. **The generator runs in the frontend** (`crypto.getRandomValues`), fixed at 32 letters and digits, no options.
4. **Multi-line values**: the value field becomes a masked multi-line field. Trailing line breaks are stripped on save. Enter in that field adds a line instead of saving.
5. **Tray**: closing the window hides it and does not lock; auto-lock still applies. If the tray icon cannot be created, closing quits as today.

## Steps

### A. Private socket (bug fix)

1. `src-tauri/src/transport/unix.rs`: add a test that creates directories on another thread while binding repeatedly and asserts each stays usable. Fails today.
   Then replace the umask in `bind()`: create a `0700` directory next to the endpoint, bind inside it, `chmod 0600`, rename the socket onto the endpoint, remove the directory. A leftover or foreign directory of that name makes bind fail rather than being trusted.
   Verify: new test passes; `socket_is_0600`, `stale_socket_replaced`, `regular_file_at_endpoint_is_not_deleted` still pass.
2. `src-tauri/tests/ipc.rs`: remove each test's temp directory on drop.
   Verify: `cargo test -p one-pass --test ipc` green 10 times in a row; no `op-ipc-*` left in the temp dir.

### B. Env file import

3. Move `onepass-cli/src/dotenv.rs` to `onepass-core/src/dotenv.rs` unchanged; the CLI imports it from core.
   Verify: `cargo test --workspace`; `cargo tree -p onepass-cli` shows no `argon2` or `chacha20poly1305`.
4. `src-tauri/src/env_import.rs`: `candidates`, `import`, `reference_lines`, with unit tests for every scenario in `env-import.feature` that does not need a window (statuses, replaces, all-or-nothing, notes kept, duplicates, missing name).
   Verify: `cargo test -p one-pass env_import`.
5. Commands `pick_env_file`, `import_env`, `copy_env_references` in `commands.rs`, registered in `lib.rs`; the picked path lives in `AppState`. Reading enforces 1 MiB and UTF-8 (tested through a path-taking helper). Add `tauri-plugin-dialog`.
   Verify: `cargo check`, `cargo test --workspace`.
6. UI: `VaultApi` methods, `EnvImportComponent` (checklist dialog with focus trap, then the reference lines and Copy), "Import .env" button in the browser's environment bar. Specs for: preselection, disabled rows, replaces warning, import call with chosen names, error shown, cancel.
   Verify: `npm test`.

### C. Vault usability

7. Entry search in `BrowserComponent` (search field, cross-environment results, Escape clears). Specs per scenario.
8. Generate button in `EntryEditorComponent`. Spec: 32 chars of the alphabet, two presses differ.
9. Multi-line value: masked textarea in the editor, trailing line breaks stripped on save, `white-space: pre-wrap` on the revealed value. Specs per scenario.
10. Shortcuts in `AppComponent` (Ctrl/Cmd+F, Ctrl/Cmd+L, unlocked only) and a Shortcuts section in Help. Specs per scenario.
    Verify for 7-10: `npm test` after each.

### D. Background tray

11. `src-tauri/Cargo.toml`: `tauri` feature `tray-icon`. `lib.rs`: build the tray (Open / Lock / Quit, left click opens); on `CloseRequested` hide the window only if the tray was created.
    Verify: `cargo check`, `cargo test --workspace`. The scenarios in `background-tray.feature` are manual.
12. Docs: README (close behaviour, tray, GNOME note, import, search, shortcuts), `docs/manual-test-checklist.md` (tray and import items), `AGENTS.md` architecture line for `env_import`.

### E. Finish

13. `cargo fmt`, `cargo test --workspace`, `npm test`, `npm run build`.
14. Run the `security-reviewer` agent over the diff (new plugin, new IPC commands, socket change) and the `rust-reviewer` agent over the Rust changes; fix what they find.
15. Start the app with `npm run tauri dev` and walk the import, search and tray scenarios, if a display is available in this environment. Otherwise report them as unchecked.

## Not doing

Windows transport, rewriting `.env`, generator options, a quit-or-hide setting, `op --help` exiting 0, directory fsync after vault save.
