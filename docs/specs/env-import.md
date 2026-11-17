# Env file import

Date: 2026-11-16. Reverses the "import" non-goal of the v1 design for one case: bringing an existing `.env` into the vault.

## Context and language

Bounded context: the vault browser (desktop app). Existing terms keep their meaning: **project**, **environment**, **entry**, **reference** (`op://project/environment/entry`), **literal** (a `.env` value that is not a reference), **looks secret** (the heuristic `op run` already warns with).

New terms:

- **Env file import**: reading one `.env` file and storing chosen variables as entries of one environment.
- **Candidate**: one variable of the file, as shown to the user. It has a name and a status, never a value.
- **Candidate status**:
  - `secret`: a literal that looks secret. Preselected.
  - `literal`: any other non-empty literal. Selectable, not preselected.
  - `reference`: already an `op://` reference. Not importable.
  - `empty`: empty value. Not importable.
- **Replaces**: the target environment already has an entry with the candidate's name. Never preselected.

## Invariants

- Values from the file never reach the frontend. The preview carries names, statuses and the replaces flag only.
- The frontend cannot name a file. The path comes from the native file dialog and is remembered in Rust until the import finishes.
- An import is all-or-nothing: one vault save. If any chosen name is missing from the file or not importable, nothing is stored.
- An existing entry is overwritten only when the user ticked it. Its notes are kept.
- The app never writes to the `.env` file. It shows the reference lines for the user to paste.
- The file is parsed with the same dotenv rules as `op run` (one parser, moved to `onepass-core`). Later duplicates win. Files over 1 MiB or not UTF-8 are refused.

## Shape

- `onepass-core::dotenv`: the parser and `looks_secret`, moved unchanged from `onepass-cli`. No crypto, so the CLI still links none.
- `src-tauri/src/env_import.rs` (Tauri-free, unit-tested): `candidates(text, body, project, environment)`, `import(body, project, environment, text, names)`, `reference_lines(project, environment, names)`.
- Commands (thin): `pick_env_file(project, environment)` opens the dialog, reads the file, returns the candidates or nothing when cancelled. `import_env(project, environment, names)` re-reads the remembered file and imports. `copy_env_references(project, environment, names)` puts the `NAME=op://...` lines on the clipboard as plain text.
- File dialog: `tauri-plugin-dialog`, called from Rust only. No new webview permission.
- UI: an "Import .env" button on the selected environment opens a dialog with the candidate checklist, then the reference lines.

## Not included

Rewriting the `.env` file, importing several files at once, export, other formats.
