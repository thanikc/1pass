# one-pass Desktop App Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Finish one-pass v1: the desktop app that owns the key, answers `op run` over a local socket after user approval, and provides the vault UI, on Linux, macOS and Windows.

**Architecture:** `src-tauri` gains Tauri-free modules (`session`, `broker`, `transport`, `autolock`, `clipboard`) that are tested through a fake `Host`, plus a thin Tauri layer (`host`, `commands`). `onepass-core` gains the persisted data model and validation. The Angular frontend is a thin client: one `VaultApi` service wraps every `invoke`, and components never hold secrets longer than a reveal.

**Tech Stack:** Rust 2024, Tauri 2, `sha2`, `libc`, `windows-sys`, `tauri-plugin-single-instance`, `tauri-plugin-clipboard-manager`, `proptest`; Angular 22, TypeScript 6, Vitest via `@angular/build:unit-test`.

**Spec:** `docs/superpowers/specs/2026-11-01-onepass-design.md`

## Global Constraints

- Never log, persist in plaintext, or send secrets over IPC beyond what is needed. The dialog shows "Never values". The audit log is "metadata only (project, entry names, outcome, time)".
- `onepass-cli` builds core with `default-features = false`: never crypto or vault code.
- Don't hand-edit `package-lock.json`, `Cargo.lock` or `src-tauri/gen/`. Change them only through `npm`/`cargo` commands.
- Rust commands: no `unwrap` outside tests, typed `onepass_core::Error` across every boundary, `Debug` never prints a value.
- Angular: standalone components, `signal()`, `inject()`, `@if`/`@for`, no `any`. Only `VaultApi` calls `invoke`. No `[innerHTML]`, no `localStorage`.
- Capabilities stay minimal. No plugin JS permission is granted when Rust can do the work.
- Use the context7 MCP server before using an unfamiliar Angular 22, Tauri 2 or TS 6 API.
- Run the `security-reviewer` agent after crypto, IPC and capability changes (marked in tasks).
- **Executor stops:** per `AGENTS.md`, a permission restriction, missing password or missing tool means stop and ask, even where the executing skill would otherwise rule and continue.
- Cargo is not on PATH in non-interactive shells. Prefix cargo commands with `. "$HOME/.cargo/env" &&`.

## Prerequisites (user action)

`cargo check -p one-pass` fails today: `pkg-config` and the Tauri system libraries are not installed. Every task from Task 3 on needs them. Before Task 1, the user runs:

```bash
sudo apt install pkg-config libdbus-1-dev libwebkit2gtk-4.1-dev libxdo-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev
```

Baseline at planning time: `cargo test -p onepass-core -p onepass-cli` gives 17 passing tests (8 core, 7 CLI unit, 2 e2e).

## Decisions Beyond the Spec

The spec is silent or conflicting on these. The plan pins them, and the user should confirm them when reviewing.

| # | Decision | Why |
|---|----------|-----|
| D1 | Add `variables: {name: reference}` to `Request`, staying on protocol version 1 | The spec's dialog must show variable names, but its request has only references. Nothing has shipped, so no version bump. |
| D2 | Replace `Body.grants: Vec<String>` with `remembered: Vec<GrantRecord>` (hash plus metadata) | The grants list with revoke cannot display a bare hash. The metadata stays inside the encrypted vault. |
| D3 | Audit log lives inside the encrypted vault, capped at 1000 records. Records made while locked buffer in memory until unlock and are lost if the app quits first. | Project and entry names should not sit in plaintext on disk. |
| D4 | The server owns the deadlines: 60 s for unlock (spec), 120 s per prompt, 180 s per request. The CLI read timeout becomes 185 s. | The current 60 s CLI read timeout would kill a prompt shown after a late unlock. |
| D5 | Entry, environment and project names match `[A-Za-z0-9._-]{1,128}`, enforced in `Reference::parse` | Guarantees Copy Reference always produces a reference the CLI and dotenv parser accept. |
| D6 | Approval runs before lookup, so a missing entry is reported only after Allow. No grant is stored if the lookup fails. | Otherwise any local process could probe entry names without approval. |
| D7 | Timeouts count toward the "3 denials in a minute" limit. Client disconnects do not. | An unattended dialog is an unapproved request; a Ctrl-C is not. |
| D8 | The approval dialog has a mode selector. Allow saves the choice as the project's mode. Changing a project's mode revokes its grants. Modes are keyed by canonical `project_dir`. | Gives "selectable per project" without a separate setup step. |
| D9 | Auto-lock default 15 min, options 1/5/15/30/60 or never. Master password minimum 8 characters. | The spec gives no values. |
| D10 | Closing the window quits the app (no tray). `op` then reports "cannot reach the one-pass app". | Tray is not in the spec. Time-boxed grants already die on quit. |
| D11 | `op` is installed with `cargo install --path onepass-cli`, not bundled. The README warns that the name collides with 1Password's `op`. | Packaging is not in the spec. |

## Review Focus

1. Client exits (Ctrl-C) while its dialog is open: the dialog disappears, no grant is stored, and it is not counted as a denial. Pinned in Tasks 5 and 6.
2. Laptop sleeps past a grant's expiry or the auto-lock timeout: on wake the grant is expired and the vault locks, so wall-clock time is used, not `Instant`. Pinned in Tasks 3 and 7.
3. Client-claimed strings with newlines, bidi overrides or huge length: the dialog cannot be made to display something misleading. Pinned in Task 5.
4. Disk full or read-only during a save: memory stays equal to disk, the error is shown, and a failed password change keeps the old password working. Pinned in Task 4.
5. Parallel `op run` for the same key (for example `npm-run-all`): one dialog, and the queued requests use the resulting grant. Pinned in Task 5.

---

## File Structure

```
onepass-core/src/lib.rs          Error: + RateLimited, Busy, InvalidInput
onepass-core/src/protocol.rs     + Request.variables, validate_name, endpoint()
onepass-core/src/vault.rs        + persisted types, Body operations, proptests
onepass-cli/src/main.rs          build_request(), 185 s wait, endpoint()
src-tauri/src/session.rs         Vault lifecycle, rollback-on-failed-save, audit buffer
src-tauri/src/broker/mod.rs      Request pipeline, Host trait, limits
src-tauri/src/broker/grants.rs   grant_key, GrantStore, DenialTracker
src-tauri/src/broker/view.rs     PromptView and display sanitizing
src-tauri/src/transport/mod.rs   Conn trait, handle_conn, connection cap
src-tauri/src/transport/unix.rs  Socket, SO_PEERCRED / LOCAL_PEERPID
src-tauri/src/transport/windows.rs  Named pipe
src-tauri/src/autolock.rs        Idle timer logic
src-tauri/src/clipboard.rs       Clear-after-30 s logic
src-tauri/src/host.rs            Tauri Host: events, pending prompts, raise window
src-tauri/src/commands.rs        #[tauri::command] layer
src-tauri/src/lib.rs             Wiring
src-tauri/tests/ipc.rs           Real-socket integration tests
src/app/api/{types,errors,vault-api}.ts
src/app/screens/{unlock,browser,approvals,audit,settings}/
docs/manual-test-checklist.md
```

---

### Task 1: Core data model and validation

**Files:**
- Modify: `onepass-core/src/lib.rs`, `onepass-core/src/protocol.rs`, `onepass-core/src/vault.rs`, `onepass-core/Cargo.toml`
- Modify (compile fixes for the new `Request` field): `onepass-cli/src/main.rs`, `onepass-cli/tests/e2e.rs`

**Interfaces:**
- Produces, in `lib.rs`: `Error::{RateLimited, Busy, InvalidInput(String)}`.
- Produces, in `protocol.rs`:
  - `Request.variables: BTreeMap<String, String>` with `#[serde(default)]`
  - `pub fn validate_name(&str) -> Result<(), Error>`
  - `pub fn endpoint() -> Result<PathBuf, Error>`
- Produces, in `vault.rs` (all `Serialize, Deserialize, Clone, Debug`; enums `snake_case`):
  - `enum ApprovalMode { PerRun, #[default] TimeBoxed, Remember }`
  - `struct GrantRecord { hash, project_dir, command_name: String, references: Vec<String>, created_at: u64 }`
  - `enum AuditOutcome { Approved, ApprovedByGrant, Denied, TimedOut, Cancelled, RateLimited, Busy, Locked, EntryNotFound, Invalid }`
  - `struct AuditRecord { at: u64, project_dir: String, entries: Vec<String>, omitted: usize, outcome: AuditOutcome }`
  - `struct Settings { auto_lock_minutes: u32 }`, default 15
  - `struct EntryMeta { project, environment, name, kind, notes: String }`
  - `Body` fields, all `#[serde(default)]`: `projects`, `remembered: Vec<GrantRecord>`, `project_modes: BTreeMap<String, ApprovalMode>`, `settings: Settings`, `audit: Vec<AuditRecord>`. Field `grants` is removed.
  - `Body` methods:
    - `create_environment(&mut self, project: &str, environment: &str) -> Result<(), Error>`
    - `save_entry(&mut self, project: &str, environment: &str, name: &str, rename_from: Option<&str>, value: Option<Zeroizing<String>>, notes: String) -> Result<(), Error>`
    - `delete_entry(&mut self, project: &str, environment: &str, name: &str) -> Result<(), Error>`
    - `delete_environment(&mut self, project: &str, environment: &str) -> Result<(), Error>`
    - `metadata(&self) -> Vec<EntryMeta>`
    - `value(&self, project: &str, environment: &str, name: &str) -> Result<&Zeroizing<String>, Error>`
    - `push_audit(&mut self, record: AuditRecord)`

- [ ] **Step 1: Write failing tests**

In `protocol.rs`:
- `names_outside_charset_rejected`: `Reference::parse` errors with `BadReference` for `"op://a b/c/d"`, `"op://a#b/c/d"`, and a 129-character project. It accepts `"op://my.app/dev-1/API_KEY"`.
- `request_without_variables_still_parses`: JSON with no `variables` key gives an empty map.
- `endpoint_prefers_env_override` (unix): with `ONEPASS_SOCKET=/x/y`, `endpoint()` returns `/x/y`. Serialize env access inside one test.

In `vault.rs`:
- `save_entry_creates_path_and_keeps_value_when_none`: save with `Some("v")`, save again with `None` and new notes; the value is still `"v"` and notes are updated.
- `save_entry_rename_moves_and_refuses_collision`: renaming `A` to `B` removes `A`. Renaming `A` onto an existing `B` gives `InvalidInput` and both are unchanged.
- `save_entry_rejects_bad_names`: `"a/b"` and `""` give `InvalidInput`.
- `delete_environment_prunes_empty_project`: removing a project's last environment removes the project.
- `metadata_has_no_values`: `serde_json::to_string(&body.metadata())` does not contain the secret `"s3cret"`.
- `audit_is_capped_at_1000`: after 1001 `push_audit` calls, length is 1000 and the oldest record is gone.
- `body_without_new_fields_opens`: `{"projects":{}}` deserializes with defaults. `settings.auto_lock_minutes == 15`.
- `prop_seal_open_round_trip` (proptest): arbitrary valid names and values survive `seal` then `open`.
- `prop_open_never_panics` (proptest): arbitrary bytes and truncated sealed vaults return `Err`, never panic. Use `FAST` KDF params.

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -p onepass-core`
Expected: compile errors for the missing items.

- [ ] **Step 3: Implement**

Add `proptest = "1"` as a dev-dependency with `cargo add`.

Rules for `endpoint()`:
- `ONEPASS_SOCKET` wins on every platform.
- Linux: `$XDG_RUNTIME_DIR/onepass.sock`. If unset, `Error::Io("set ONEPASS_SOCKET or XDG_RUNTIME_DIR")`. Do not fall back to a world-writable directory.
- macOS: `$TMPDIR/onepass.sock`, which is per-user.
- Windows: `\\.\pipe\onepass-<USERNAME>`.

`Reference::parse` calls `validate_name` on all three parts and maps a failure to `BadReference(s)`. `validate_name` enforces D5 and returns `InvalidInput`. `push_audit` keeps at most 1000 records, dropping the oldest. Add `variables: BTreeMap::new()` wherever a `Request` is constructed, including the CLI and its tests.

- [ ] **Step 4: Run to verify they pass**

Run: `cargo test -p onepass-core -p onepass-cli`
Expected: all pass, including the existing 17.

- [ ] **Step 5: Commit**

`git commit -m "feat(core): add grant, audit and settings model with name validation"`

---

### Task 2: CLI request building and deadlines

**Files:**
- Modify: `onepass-cli/src/main.rs`, `onepass-cli/tests/e2e.rs`

**Interfaces:**
- Consumes: `protocol::endpoint`, `Request.variables`.
- Produces: `fn build_request(vars: &BTreeMap<String, String>, command: &[String], project_dir: String) -> Result<Option<Request>, Error>`. Returns `None` when no value is a reference.

- [ ] **Step 1: Write failing tests** in the existing `tests` module

- `build_request_maps_variables_and_omits_literals`: vars `{A: op://p/e/n, B: op://p/e/n, C: literal}`. `references == ["op://p/e/n"]`, `variables` has exactly `A` and `B`, and the serialized request does not contain `"literal"`.
- `build_request_none_without_references`.
- `build_request_rejects_malformed_reference`: `{A: "op://bad"}` gives `Err(BadReference)`.

In `tests/e2e.rs`, for the spec's "Exit codes and signals are forwarded":
- `exit_code_is_forwarded`: `sh -c 'exit 7'` gives status code 7.
- `signal_death_is_forwarded`: `sh -c 'kill -TERM $$'` gives `status.signal() == Some(15)`, which the current `128 + sig` shortcut fails.

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -p onepass-cli`
Expected: compile error, `build_request` not found. Once it compiles, `signal_death_is_forwarded` fails.

- [ ] **Step 3: Implement**

Add `build_request` and call it from `main`. Set `WAIT` to 185 s, with a comment that the server owns the real deadlines. Replace the path logic in `connect()` with `protocol::endpoint()`. On Unix, launch the child with `CommandExt::exec`, so signals reach it directly and its exit status is `op`'s; an `exec` failure goes through `die`. Non-Unix keeps spawn-and-forward. Remove the `shortcut:` comment.

- [ ] **Step 4: Run to verify they pass**

Run: `cargo test -p onepass-cli`
Expected: all pass, including `e2e`.

- [ ] **Step 5: Commit**

`git commit -m "feat(cli): send variable names and let the server own deadlines"`

---

### Task 3: Grants and denial tracking

**Files:**
- Create: `src-tauri/src/broker/mod.rs` (module declarations only for now), `src-tauri/src/broker/grants.rs`
- Modify: `src-tauri/src/lib.rs`, `src-tauri/Cargo.toml`

**Interfaces:**
- Consumes: `GrantRecord`.
- Produces:
  - `pub fn grant_key(project_dir: &str, references: &[String], command: &[String]) -> String`: lowercase hex SHA-256
  - `pub fn command_name(command: &[String]) -> Option<String>`: the file name of `command[0]`
  - `pub enum GrantDuration { Min15, Hour1, UntilLock }`: serde `snake_case`
  - `pub struct ActiveGrant { pub record: GrantRecord, pub expires_at: Option<SystemTime> }`
  - `GrantStore`: `is_granted(&self, key: &str, now: SystemTime) -> bool`, `insert(&mut self, record: GrantRecord, duration: GrantDuration, now: SystemTime)`, `revoke(&mut self, hash: &str) -> bool`, `revoke_project(&mut self, project_dir: &str)`, `clear(&mut self)`, `list(&self, now: SystemTime) -> Vec<ActiveGrant>`
  - `DenialTracker`: `record(&mut self, project_dir: &str, now: SystemTime)`, `is_limited(&self, project_dir: &str, now: SystemTime) -> bool`

- [ ] **Step 1: Write failing tests** in `grants.rs`

- `key_ignores_reference_order_and_duplicates`: `[a,b]`, `[b,a]` and `[a,b,a]` give the same key.
- `key_changes_with_each_input`: differing dir, reference set, or exec name give different keys. `/usr/bin/node` and `node` give the same key; arguments do not affect the key.
- `key_fields_are_unambiguous`: references `["ab","c"]` and `["a","bc"]` differ.
- `min15_expires_by_wall_clock`: granted at `now + 14 min`, not at `now + 15 min`.
- `grant_expired_after_sleep`: at `now + 8 h`, `is_granted` is false and `list` is empty.
- `until_lock_lasts_until_clear`: granted at `now + 30 days`, not after `clear()`.
- `revoke_project_only_touches_that_project`.
- `third_denial_limits_within_a_minute`: two denials are not limited. Three within 60 s are limited at +59 s and not limited at +61 s after the first, since the window slides.
- `limit_is_per_project`.

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -p one-pass broker::grants`
Expected: compile errors.

- [ ] **Step 3: Implement**

Run `cargo add -p one-pass sha2 zeroize` and `cargo add -p one-pass onepass-core --path onepass-core`. The hash input is length-prefixed fields (`u64` LE length, then bytes) for dir, exec name, then the sorted deduplicated references. Use `SystemTime` throughout, never `Instant`. `duration_since` errors count as not expired.

- [ ] **Step 4: Run to verify they pass**

Run: `cargo test -p one-pass broker::grants`
Expected: PASS.

- [ ] **Step 5: Commit**

`git commit -m "feat(app): add grant keys, grant store and denial tracker"`

---

### Task 4: Session

**Files:**
- Create: `src-tauri/src/session.rs`
- Modify: `src-tauri/src/lib.rs`

**Interfaces:**
- Consumes: `GrantStore` and `ActiveGrant` from Task 3; `Vault`, `Body`, `KdfParams`, `AuditRecord` from core.
- Produces:
  - `pub type Cancel = Arc<dyn Fn() -> bool + Send + Sync>`
  - `pub enum VaultStatus { Missing, Locked, Unlocked }`: serde `snake_case`
  - `impl Session`:
    - `new(path: PathBuf, kdf: KdfParams) -> Self`
    - `status(&self) -> VaultStatus`
    - `create(&self, password: &[u8]) -> Result<(), Error>`
    - `unlock(&self, password: &[u8]) -> Result<(), Error>`
    - `lock(&self)`
    - `change_password(&self, current: &[u8], new: &[u8]) -> Result<(), Error>`
    - `read<T>(&self, f: impl FnOnce(&Vault) -> T) -> Result<T, Error>`
    - `mutate<T>(&self, f: impl FnOnce(&mut Body) -> Result<T, Error>) -> Result<T, Error>`
    - `wait_unlocked(&self, timeout: Duration, cancel: &Cancel) -> bool`
    - `is_granted(&self, key: &str, now: SystemTime) -> bool`: time-boxed or remembered
    - `add_timeboxed(&self, record: GrantRecord, duration: GrantDuration, now: SystemTime)`
    - `remember(&self, record: GrantRecord) -> Result<(), Error>`
    - `revoke_grant(&self, hash: &str) -> Result<(), Error>`
    - `list_grants(&self, now: SystemTime) -> Result<Vec<ActiveGrant>, Error>`: remembered ones have `expires_at: None`
    - `project_mode(&self, dir: &str) -> Result<ApprovalMode, Error>`
    - `set_project_mode(&self, dir: &str, mode: ApprovalMode) -> Result<(), Error>`
    - `record_audit(&self, record: AuditRecord)`

- [ ] **Step 1: Write failing tests** in `session.rs`, using a temp dir and `FAST` KDF

- `status_transitions`: `Missing`, then create gives `Unlocked`, lock gives `Locked`, unlock gives `Unlocked`.
- `create_refuses_existing_file`: a file with garbage bytes at the path. `create` gives `InvalidInput`, the bytes are unchanged, `status` is `Locked`, and `unlock` gives `VaultCorrupt`.
- `create_refuses_short_password`: 7 bytes gives `InvalidInput`.
- `read_and_mutate_when_locked_are_typed`: both give `VaultLocked`.
- `mutate_persists`: after mutate, lock and unlock, the change is present.
- `failed_save_rolls_back_memory`: make the save fail by creating a directory at the temp-file path (`<path with .tmp extension>`). `mutate` returns `Err(Io)` and a subsequent `read` shows the old body. Remove the directory; a later mutate succeeds.
- `failed_change_password_keeps_old`: same failure injection. `change_password` errors, and after lock the old password unlocks and the new one gives `WrongPassword`.
- `change_password_requires_current`: wrong current gives `WrongPassword` and nothing changes.
- `lock_clears_timeboxed_grants_and_keeps_remembered`.
- `set_project_mode_revokes_project_grants`: both time-boxed and remembered.
- `audit_while_locked_is_flushed_on_unlock`: `record_audit` while locked, then unlock. `read` shows the record.
- `wait_unlocked_wakes_on_unlock`, `wait_unlocked_times_out`, `wait_unlocked_stops_on_cancel`.

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -p one-pass session`
Expected: compile errors.

- [ ] **Step 3: Implement**

- `mutate` clones `body`, applies `f`, saves, and restores the clone if the save fails.
- `change_password` runs `Vault::load(path, current)`, then `change_password(new)`, then `save`, and only then swaps it into memory. Memory is therefore untouched on any failure.
- Internal wait polling (100 ms `Condvar::wait_timeout`) re-checks `cancel`.
- `create` calls `create_dir_all` on the parent.
- The buffered audit list holds at most 1000 records.
- Never hold the vault mutex while waiting on anything.

- [ ] **Step 4: Run to verify they pass**

Run: `cargo test -p one-pass session`
Expected: PASS.

- [ ] **Step 5: Commit**

`git commit -m "feat(app): add vault session with rollback on failed save"`

---

### Task 5: Broker pipeline

**Files:**
- Modify: `src-tauri/src/broker/mod.rs`
- Create: `src-tauri/src/broker/view.rs`

**Interfaces:**
- Consumes: Tasks 3 and 4.
- Produces, in `view.rs`:
  - `pub struct PromptView { pub id: u64, pub project_dir: String, pub command: Vec<String>, pub command_name: String, pub peer_pid: Option<u32>, pub peer_exe: Option<String>, pub variables: Vec<(String, String)>, pub references: Vec<String>, pub mode: ApprovalMode }`: `Serialize`, camelCase. `id` is assigned by `Host`; `Broker` passes 0.
  - `pub fn sanitize(s: &str, max_chars: usize) -> String`
- Produces, in `mod.rs`:
  - `pub struct Peer { pub pid: Option<u32>, pub exe: Option<String> }`
  - `pub enum Decision { Allow { mode: ApprovalMode, duration: GrantDuration }, Deny, TimedOut, Cancelled }`
  - `pub trait Host: Send + Sync + 'static { fn request_unlock(&self); fn prompt(&self, view: PromptView, timeout: Duration, cancel: Cancel) -> Decision; }`
  - `pub struct Limits { pub unlock_wait: Duration, pub approval_wait: Duration, pub request_deadline: Duration }`: `Default` 60 s, 120 s, 180 s
  - `Broker::new(session: Arc<Session>, host: Arc<dyn Host>, limits: Limits) -> Self`
  - `Broker::handle(&self, raw: &[u8], peer: Peer, client_gone: Cancel) -> Response`
  - `pub const MAX_PENDING: usize = 8`, `MAX_AUDIT_ENTRIES: usize = 64`

**Pipeline order, which is a requirement:**

1. Parse a `{version}` probe first; a mismatch gives `VersionMismatch`. Unparseable gives `Io("malformed request")`.
2. Parse the full `Request`. Validate:
   - `command` is non-empty and `command_name` exists
   - every reference passes `Reference::parse`
   - the set of `variables` values equals the set of `references`, otherwise `InvalidInput`
   - canonicalize `project_dir`; failure gives `InvalidInput("project directory not found")`
   - sort and dedupe references

   An empty reference set returns `Ok({})` with no prompt.
3. If `DenialTracker::is_limited`: `RateLimited`.
4. If locked: `host.request_unlock()`, then `wait_unlocked(unlock_wait)`. On timeout: `VaultLocked`.
5. Unless mode is `PerRun`: if `is_granted`, skip to step 8 with outcome `ApprovedByGrant`.
6. Pending count at `MAX_PENDING`: `Busy`. Otherwise enter the prompt queue (one prompt at a time, a `Condvar` gate that honors the deadline and cancel). On leaving the queue, re-run steps 3 and 5.
7. `host.prompt` with timeout `min(approval_wait, remaining deadline)` and a cancel combining `client_gone` and "vault locked".
   - `Deny` and `TimedOut`: record a denial, return `Denied`.
   - `Cancelled` while locked: `VaultLocked`. `Cancelled` by the client: `Denied` with outcome `Cancelled`, no denial recorded.
8. `session.read(|v| v.resolve(&req))`. Only if this succeeds, store the grant by the chosen mode and persist a changed mode (`Remember` uses `remember`). A mode change goes through `set_project_mode` before the grant is added.
9. `record_audit` on every path, with at most `MAX_AUDIT_ENTRIES` references plus `omitted`, and `project_dir` truncated to 1024 bytes. An audit save failure never changes the response.

- [ ] **Step 1: Write failing tests** in `broker/mod.rs`, with a `FakeHost` (scripted decisions, prompt counter, unlock counter) and short `Limits`

Each drives `handle` with JSON bytes:

- `allow_returns_only_requested_values`: vault has `A` and `B`, request `A`. Map has one key with the right value.
- `deny_returns_denied_and_prompts_once`.
- `version_mismatch_before_anything`: `{"version":99}` gives `VersionMismatch { expected: 1, got: 99 }` and 0 prompts.
- `malformed_is_typed`: `b"nope"` gives `Io`.
- `variables_must_match_references`: a reference absent from `variables` gives `InvalidInput` and 0 prompts.
- `nonexistent_project_dir_rejected`.
- `missing_entry_prompts_then_not_found_and_stores_no_grant`: allow, get `EntryNotFound`. A repeat prompts again.
- `locked_raises_window_and_times_out`: `VaultLocked`, unlock counter 1, 0 prompts.
- `locked_then_unlocked_proceeds`: unlock from another thread during the wait. Allowed.
- `timeboxed_grant_skips_second_prompt`, `changed_reference_set_prompts_again`, `changed_exec_prompts_again`.
- `per_run_always_prompts`: allow twice, 2 prompts.
- `remember_survives_lock`: allow with `Remember`, lock, unlock. No new prompt.
- `three_denials_rate_limit`: 4th request gives `RateLimited` and the prompt count stays 3.
- `timeout_counts_as_denial`.
- `client_disconnect_cancels_without_grant_or_denial`: `client_gone` turns true during the prompt. No grant, no denial, audit `Cancelled`.
- `lock_during_prompt_returns_locked`.
- `pending_cap_returns_busy`: with a blocking host and 8 pending, the 9th gets `Busy` immediately.
- `parallel_same_key_prompts_once`: 4 threads, same request, host allows time-boxed. All get values, prompt count 1.
- `queued_requests_respect_rate_limit`: 5 threads, host denies. Prompt count is 3, the rest get `RateLimited`.
- `audit_has_no_values`: after an allow, `serde_json::to_string(&body.audit)` does not contain the secret, and the outcome is `Approved`.
- `prompt_view_has_no_values`: serialized `PromptView` does not contain the secret.

In `view.rs`:

- `sanitize_replaces_control_and_bidi`: `"a\nb\u{202E}c\u{2066}"` gives `"a\u{FFFD}b\u{FFFD}c\u{FFFD}"`.
- `sanitize_truncates_with_ellipsis`: 5000 chars with max 1024 gives 1024 chars ending in `…`.
- `huge_argv_is_bounded`: more than 64 args show 64 plus a final `"… (N more)"`.

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -p one-pass broker`
Expected: compile errors.

- [ ] **Step 3: Implement** `Broker` and `view.rs` per the pipeline above

Sanitize replaces Unicode `Cc`, and bidi controls U+061C, U+200E, U+200F, U+202A–U+202E, U+2066–U+2069, with U+FFFD. `project_dir` and each arg max 1024 chars.

- [ ] **Step 4: Run to verify they pass**

Run: `cargo test -p one-pass broker`
Expected: PASS. Repeat 5 times to catch flakes in the threaded tests.

- [ ] **Step 5: Commit**

`git commit -m "feat(app): add approval broker pipeline"`

---

### Task 6: Unix transport

**Files:**
- Create: `src-tauri/src/transport/mod.rs`, `src-tauri/src/transport/unix.rs`, `src-tauri/tests/ipc.rs`
- Modify: `src-tauri/src/lib.rs`, `src-tauri/Cargo.toml`

**Interfaces:**
- Consumes: `Broker::handle`, `Peer`, `Cancel`.
- Produces:
  - `pub const MAX_REQUEST: usize = 64 * 1024`, `MAX_CONNECTIONS: usize = 32`, `READ_TIMEOUT: Duration` (5 s)
  - `pub trait Conn: Read + Write + Send + 'static { fn peer(&self) -> Peer; fn client_gone(&self) -> Cancel; }`
  - `pub fn handle_conn<C: Conn>(broker: &Broker, conn: C)`
  - `pub fn serve(broker: Arc<Broker>, endpoint: PathBuf) -> Result<ServerHandle, Error>`
  - `ServerHandle::shutdown(self)`: stops accepting and removes the socket
- `one_pass_lib` must expose `broker`, `session` and `transport` as `pub mod` for the integration tests.

- [ ] **Step 1: Write failing tests** in `src-tauri/tests/ipc.rs`

Real socket, fake `Host`, temp dir. A `#![cfg(unix)]` file. A helper sends a `Request` and parses the `Response`.

Spec scenarios: `allow`, `deny`, `locked`, `rate_limit` (over 4 connections), `changed_reference_set`.

Hardening:
- `socket_is_0600`: `metadata.mode() & 0o777 == 0o600`.
- `peer_pid_and_exe_reach_prompt`: the host sees `peer_pid == Some(std::process::id())` and `peer_exe` is non-empty.
- `oversized_request_rejected_without_prompt`: 64 KiB + 1 bytes gives `InvalidInput`, 0 prompts.
- `slow_client_does_not_wedge_server`: an idle connection that sends nothing; a second client is still served, and with a short `READ_TIMEOUT` override the first is closed.
- `connection_cap`: with 32 idle connections, the 33rd is closed with no response, and the server recovers once they close.
- `stale_socket_replaced`: a leftover socket file with no listener, `serve` succeeds.
- `live_socket_refused`: a second `serve` on the same path gives `Err` and the first keeps working.
- `client_exit_cancels_prompt`: the client drops mid-prompt; the host's `cancel` becomes true within 1 s.
- `connection_closes_after_response`: the client's next read returns EOF.

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -p one-pass --test ipc`
Expected: compile errors.

- [ ] **Step 3: Implement** `transport/mod.rs` and `transport/unix.rs`

- `cargo add -p one-pass libc --target 'cfg(unix)'`. Allow a test override of `READ_TIMEOUT` through a `serve_with` variant.
- `handle_conn` reads to the first newline, up to `MAX_REQUEST`. After responding it serializes into `Zeroizing<Vec<u8>>` and zeroizes each response value.
- Bind: try `connect` first. If it succeeds, return `Err(Io("another instance is running"))`. Otherwise remove the stale file, bind, and `chmod` to `0600`.
- Peer: reject on uid mismatch. Linux uses `SO_PEERCRED` and `/proc/<pid>/exe`. macOS uses `LOCAL_PEERCRED`/`getpeereid`, `LOCAL_PEERPID` and `libc::proc_pidpath`. Failures yield `None`, as the data is informational.
- `client_gone`: `libc::recv` with `MSG_PEEK | MSG_DONTWAIT` returns 0 or a hard error.
- One thread per connection. An over-cap connection is dropped immediately.

- [ ] **Step 4: Run to verify they pass**

Run: `cargo test -p one-pass --test ipc`
Expected: PASS.

- [ ] **Step 5: Run the `security-reviewer` agent** on the diff since Task 3 and fix Critical and Important findings with a failing test first

- [ ] **Step 6: Commit**

`git commit -m "feat(app): add unix socket transport with peer lookup"`

---

### Task 7: Auto-lock and clipboard logic

**Files:**
- Create: `src-tauri/src/autolock.rs`, `src-tauri/src/clipboard.rs`
- Modify: `src-tauri/src/lib.rs`

**Interfaces:**
- Produces:
  - `AutoLock`: `new(now: SystemTime)`, `touch(&mut self, now: SystemTime)`, `is_due(&self, now: SystemTime, minutes: u32) -> bool`
  - `pub trait Clipboard { fn write_secret(&self, text: &str) -> Result<(), Error>; fn write_plain(&self, text: &str) -> Result<(), Error>; fn read(&self) -> Option<Zeroizing<String>>; fn clear(&self); }`
  - `ClipboardGuard`: `copy_secret(&mut self, cb: &dyn Clipboard, value: &str, now: SystemTime) -> Result<(), Error>`, `tick(&mut self, cb: &dyn Clipboard, now: SystemTime)`, `clear_now(&mut self, cb: &dyn Clipboard)`

- [ ] **Step 1: Write failing tests**

`autolock.rs`:
- `due_after_timeout`: at 15 min with `minutes = 15`, due; at 14 min, not.
- `zero_means_never`.
- `touch_resets`.
- `due_after_system_sleep`: `now + 8 h` is due. Clock moved backwards is not due and does not panic.

`clipboard.rs`, with a fake clipboard:
- `cleared_after_30s`: tick at 29 s leaves the value, tick at 30 s clears.
- `user_overwrite_is_not_cleared`: the fake holds other text at 30 s, and it is untouched.
- `second_copy_restarts_timer`.
- `clear_now_only_clears_ours`.
- `guard_stores_digest_only`: `format!("{guard:?}")` does not contain the value. The guard holds a SHA-256 digest, not the secret.

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -p one-pass autolock` and `cargo test -p one-pass clipboard`
Expected: compile errors.

- [ ] **Step 3: Implement** both modules

- [ ] **Step 4: Run to verify they pass**

Run: `cargo test -p one-pass autolock` and `cargo test -p one-pass clipboard`
Expected: PASS.

- [ ] **Step 5: Commit**

`git commit -m "feat(app): add auto-lock and clipboard clearing logic"`

---

### Task 8: Tauri commands and wiring

**Files:**
- Create: `src-tauri/src/host.rs`, `src-tauri/src/commands.rs`
- Modify: `src-tauri/src/lib.rs`, `src-tauri/Cargo.toml`, `src-tauri/capabilities/default.json`, `package.json`

**Interfaces:**
- Consumes: everything above.
- Produces, in `host.rs`: `TauriHost` implementing `Host`, plus `pending(&self) -> Vec<PromptView>` and `respond(&self, id: u64, decision: Decision) -> Result<(), Error>`. Events have no payload: `vault-state-changed`, `approvals-changed`.
- Produces, in `commands.rs` (arguments camelCase in JS; all return `Result<T, Error>`; all `#[tauri::command(async)]`, so Argon2 and locks never block the main thread):

| Command | Args | Returns |
|---|---|---|
| `vault_status` | | `VaultStatus` |
| `create_vault` | `password` | `()` |
| `unlock` | `password` | `()` |
| `lock` | | `()` |
| `change_password` | `current, next` | `()` |
| `list_entries` | | `Vec<EntryMeta>` |
| `list_environments` | | `Vec<(String, String)>`: includes empty ones |
| `create_environment` | `project, environment` | `()` |
| `save_entry` | `project, environment, name, renameFrom, value, notes` | `()` |
| `delete_entry` | `project, environment, name` | `()` |
| `delete_environment` | `project, environment` | `()` |
| `reveal_entry` | `project, environment, name` | `String` |
| `copy_entry_value` | `project, environment, name` | `()` |
| `copy_reference` | `project, environment, name` | `()` |
| `list_pending_approvals` | | `Vec<PromptView>` |
| `respond_approval` | `id, decision` | `()` |
| `list_grants` | | `Vec<GrantView>`, where `GrantView { hash, projectDir, commandName, references, expiresAt: Option<u64>, remembered: bool }` |
| `revoke_grant` | `hash` | `()` |
| `list_project_modes` | | `BTreeMap<String, ApprovalMode>` |
| `set_project_mode` | `projectDir, mode` | `()` |
| `list_audit` | | `Vec<AuditRecord>`: newest first |
| `get_settings` | | `Settings` |
| `set_auto_lock` | `minutes` | `()`: one of 0, 1, 5, 15, 30, 60, otherwise `InvalidInput` |
| `note_activity` | | `()` |
| `get_ipc_error` | | `Option<String>`: set if the socket could not be bound |

Passwords and `value` arrive as `Zeroizing<String>`. `reveal_entry` is the only command returning a value. `respond_approval`'s `decision` is a `DecisionInput` with `#[serde(rename_all = "snake_case")]`, wire shape `{ allow: { mode, duration } } | "deny"`; the frontend cannot send `TimedOut` or `Cancelled`.

- [ ] **Step 1: Write failing tests** in `host.rs`

Use a `PromptQueue` type holding the pending map and channels, separate from `AppHandle`, so it can be tested:

- `respond_delivers_decision`.
- `respond_unknown_id_is_error`: a second respond to the same id errors.
- `prompt_times_out_and_is_removed`.
- `prompt_cancel_removes_view`.
- `ids_are_not_reused`.

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -p one-pass host`
Expected: compile errors.

- [ ] **Step 3: Implement** `host.rs`, then `commands.rs`

- `cargo add -p one-pass tauri-plugin-single-instance tauri-plugin-clipboard-manager`.
- Remove the template's `greet` and opener: `cargo remove -p one-pass tauri-plugin-opener` and `npm uninstall @tauri-apps/plugin-opener`.
- `Clipboard` impl uses `ClipboardExt` from Rust. Check with context7 and the plugin source whether the resolved version exposes `write_secret` in Rust, and use it for secrets if so, otherwise `write_text`.
- `request_unlock` and `prompt` call `unminimize`, `show` and `set_focus` on window `main` and emit the event.

- [ ] **Step 4: Wire `lib.rs`**

- Register `single-instance` first, with a callback that raises `main`.
- In `setup`: resolve `app_data_dir()`, build `Session` for `vault.onepass` with `KdfParams::default()`, build `Broker`, and call `serve(endpoint())`. A failed bind shows the error in the UI status rather than panicking (`vault_status` still works; report it through `get_ipc_error`).
- Spawn one 1 s maintenance thread: on `AutoLock::is_due` run `lock` then `ClipboardGuard::clear_now` and emit `vault-state-changed`; always `ClipboardGuard::tick`.
- On `RunEvent::Exit`: clear our clipboard value and call `ServerHandle::shutdown`.
- `capabilities/default.json` permissions become exactly `["core:event:default"]`. Confirm `listen` works with only this, and add nothing else without a stated reason.

- [ ] **Step 5: Verify**

Run: `cargo test -p one-pass` and `cargo check --manifest-path src-tauri/Cargo.toml`
Expected: all pass, no warnings. `grep -rn "println\|eprintln\|dbg!" src-tauri/src` shows no line printing a value.

- [ ] **Step 6: Run the `security-reviewer` agent** on commands, host and capabilities

- [ ] **Step 7: Commit**

`git commit -m "feat(app): add Tauri commands, approval host and app wiring"`

---

### Task 9: Frontend foundation and unlock

**Files:**
- Create: `src/app/api/types.ts`, `src/app/api/errors.ts`, `src/app/api/vault-api.ts`, `src/app/screens/unlock/*`, `tsconfig.spec.json`
- Modify: `src/app/app.component.{ts,html,css}`, `src/app/app.config.ts`, `angular.json`, `package.json`, `src/styles.css`
- Delete: `src/app/app.routes.ts`, `src/assets/angular.svg`, `src/assets/tauri.svg`

**Interfaces:**
- Consumes: the command table from Task 8.
- Produces:
  - `types.ts`: TS mirrors of every view type.
  - `errors.ts`: `type OnePassError = "WrongPassword" | "VaultCorrupt" | "VaultLocked" | "Denied" | "RateLimited" | "Busy" | { EntryNotFound: string } | { VersionMismatch: { expected: number; got: number } } | { BadReference: string } | { InvalidInput: string } | { Io: string }`, and `errorMessage(e: unknown): string`.
  - `VaultApi`: an injectable with one typed method per command (camelCase names), plus `onVaultStateChanged(cb)` and `onApprovalsChanged(cb)`, each returning an unlisten function.
  - `AppComponent`: `status` signal from `vault_status`, refreshed on `vault-state-changed`. Shows `UnlockComponent` when `missing` or `locked`, the main shell when `unlocked`.

- [ ] **Step 1: Add the test runner**

Run: `npm install -D vitest jsdom`, add a `test` target with builder `@angular/build:unit-test` to `angular.json` (`buildTarget: one-pass:build`, `tsConfig: tsconfig.spec.json`, `runner: vitest`), and a `"test": "ng test --watch=false"` script. Check context7 for the exact options.
Expected: `npm test` runs with no spec files and reports none found.

- [ ] **Step 2: Write failing tests**

`errors.spec.ts`:
- `"WrongPassword"` gives `"Wrong password."`
- `{ EntryNotFound: "op://a/b/c" }` contains `op://a/b/c`
- unknown input gives a generic message, and never stringifies objects it doesn't recognize.

`unlock.component.spec.ts`, with a fake `VaultApi`:
- `missing` shows the create form with password and confirm.
- Mismatched confirm disables submit and never calls `createVault`.
- A password under 8 characters disables submit.
- `locked` shows only the password field.
- A wrong password shows the message and clears the input.
- A successful submit clears the input.
- Corrupt: `VaultCorrupt` shows the message and offers no create action.

`app.component.spec.ts`: renders the unlock screen when locked and the shell when unlocked; a `vault-state-changed` callback re-fetches status and removes the shell; a non-null `getIpcError` shows a banner saying `op` cannot connect.

- [ ] **Step 3: Run to verify they fail**

Run: `npm test`
Expected: FAIL, modules not found.

- [ ] **Step 4: Implement**

Remove the greet demo, the template markup and the router (`npm uninstall @angular/router`). Use no web fonts, since CSP forbids external ones; a system font stack with CSS custom properties in `styles.css`, including `prefers-color-scheme`. Use the `impeccable:impeccable` skill for the visual pass of this and later screens.

- [ ] **Step 5: Run to verify they pass**

Run: `npm test && npm run build`
Expected: PASS, build succeeds.

- [ ] **Step 6: Commit**

`git commit -m "feat(ui): add API service, test runner and unlock screen"`

---

### Task 10: Vault browser and entry editor

**Files:**
- Create: `src/app/screens/browser/browser.component.{ts,html,css,spec.ts}`, `src/app/screens/browser/entry-editor.component.{ts,html,css,spec.ts}`
- Modify: `src/app/app.component.{ts,html}`

**Interfaces:**
- Consumes: `VaultApi`.
- Produces: `BrowserComponent` (no inputs), mounted in the shell's Vault tab.

- [ ] **Step 1: Write failing tests** using a fake `VaultApi` and `vi.useFakeTimers()`

Browser:
- The sidebar groups projects and environments; selecting one filters entries.
- The detail pane shows `••••••••` by default, and `revealEntry` is not called until Reveal.
- Reveal shows the value; after 9999 ms it still shows, at 10000 ms it is masked and the signal is empty.
- Selecting another entry re-masks immediately.
- A stale reveal: selecting B while the reveal of A is in flight never displays A's value under B.
- Copy calls `copyEntryValue` and never `revealEntry`.
- Copy Reference calls `copyReference`.
- Delete requires confirmation.
- Empty vault shows a create-project prompt.
- Component destroy clears the timer and the revealed value.

Editor:
- New entry requires name and value.
- A name failing `^[A-Za-z0-9._-]{1,128}$` shows inline help and blocks save.
- Editing without touching the value sends `value: null`.
- Renaming sends `renameFrom`.
- A rename warns that references to the old name stop resolving.
- Server `InvalidInput` is displayed.
- The value input is `type="password"` with `autocomplete="off"` and `spellcheck="false"`.

- [ ] **Step 2: Run to verify they fail**

Run: `npm test`
Expected: FAIL, modules not found.

- [ ] **Step 3: Implement**

The 10 s re-hide is a constant in the component. Copy feedback states "Clipboard clears in 30 s".

- [ ] **Step 4: Run to verify they pass**

Run: `npm test && npm run build`
Expected: PASS.

- [ ] **Step 5: Commit**

`git commit -m "feat(ui): add vault browser and entry editor"`

---

### Task 11: Approval dialog, grants and modes

**Files:**
- Create: `src/app/screens/approvals/approval-dialog.component.{ts,html,css,spec.ts}`, `src/app/screens/approvals/grants.component.{ts,html,css,spec.ts}`
- Modify: `src/app/app.component.{ts,html}`

**Interfaces:**
- Consumes: `listPendingApprovals`, `respondApproval`, `listGrants`, `revokeGrant`, `listProjectModes`, `setProjectMode`, `onApprovalsChanged`.
- Produces: `ApprovalDialogComponent`, mounted at the app root above all screens so it shows over any tab; `GrantsComponent` in the Approvals tab.
- `respondApproval` decision wire shape: `{ allow: { mode, duration } } | "deny"`, the `DecisionInput` type from Task 8.

- [ ] **Step 1: Write failing tests**

Dialog:
- Fetches on init and on `approvals-changed`, so a prompt that arrived before the listener attached is still shown.
- Shows project directory, command, caller PID and executable, variable names and entry names, using a text-binding assertion: a `<b>x</b>` command renders as literal text.
- The mode selector preselects the project's mode from the view. `TimeBoxed` shows a duration selector with 15 min preselected and options 15 min, 1 h, until lock; other modes hide it.
- Allow is disabled for 500 ms after a prompt appears, and initial focus is on Deny, so a keystroke in progress cannot approve.
- With two pending, only the first shows, plus "1 more waiting"; after respond, the next appears and re-arms the 500 ms delay.
- A prompt removed server-side closes the dialog.
- `respondApproval` failing with an unknown id closes that prompt quietly.

Grants:
- Lists grants with project, command, entries, and expiry or "Until lock"/"Remembered".
- Revoke calls `revokeGrant` and refreshes.
- Project modes list changes through `setProjectMode`, showing a note that this revokes the project's grants.

- [ ] **Step 2: Run to verify they fail**

Run: `npm test`
Expected: FAIL, modules not found.

- [ ] **Step 3: Implement** both components

- [ ] **Step 4: Run to verify they pass**

Run: `npm test && npm run build`
Expected: PASS.

- [ ] **Step 5: Commit**

`git commit -m "feat(ui): add approval dialog, grants list and project modes"`

---

### Task 12: Audit log and settings

**Files:**
- Create: `src/app/screens/audit/audit.component.{ts,html,css,spec.ts}`, `src/app/screens/settings/settings.component.{ts,html,css,spec.ts}`
- Modify: `src/app/app.component.{ts,html}`

**Interfaces:**
- Consumes: `listAudit`, `getSettings`, `setAutoLock`, `changePassword`, `lock`, `noteActivity`.

- [ ] **Step 1: Write failing tests**

Audit:
- Renders time, project, entries, outcome, newest first.
- `omitted > 0` shows "+N more".
- An empty log shows an empty state.

Settings:
- The auto-lock select has values 1, 5, 15, 30, 60 and Never (0), and selecting calls `setAutoLock`.
- Change password requires current, next (at least 8 characters) and matching confirm.
- `WrongPassword` shows inline and clears the fields; success clears the fields.

`AppComponent`:
- `mousedown` and `keydown` call `noteActivity` at most once per 10 s (fake timers).
- The Lock button calls `lock`.
- The shell has tabs Vault, Approvals, Audit, Settings.

- [ ] **Step 2: Run to verify they fail**

Run: `npm test`
Expected: FAIL, modules not found.

- [ ] **Step 3: Implement** both components and the activity listener

- [ ] **Step 4: Run to verify they pass**

Run: `npm test && npm run build`
Expected: PASS.

- [ ] **Step 5: Commit**

`git commit -m "feat(ui): add audit log and settings"`

---

### Task 13: Windows named pipe

**Files:**
- Create: `src-tauri/src/transport/windows.rs`
- Modify: `src-tauri/src/transport/mod.rs`, `src-tauri/Cargo.toml`, `onepass-cli/src/main.rs`, `onepass-cli/Cargo.toml`, `onepass-cli/tests/e2e.rs`, `onepass-core/examples/dev_server.rs`

**Interfaces:**
- Consumes: `Conn`, `handle_conn`, `endpoint()`, `MAX_CONNECTIONS`, `READ_TIMEOUT`.
- Produces: `transport::serve` on Windows with the same signature and behavior as on Unix. CLI `connect()` on Windows returns a `std::fs::File`.

This cannot run on the WSL development host. It is verified by a target-specific compile here and the manual checklist on Windows (spec: "Manual: UI and Windows named pipe").

- [ ] **Step 1: Make existing Unix-only code build elsewhere**

Add `#![cfg(unix)]` to `onepass-cli/tests/e2e.rs`. Gate `dev_server` so it compiles to an empty `main` off Unix.

- [ ] **Step 2: Implement `transport/windows.rs`** with `windows-sys` (`cfg(windows)` dependency)

- `CreateNamedPipeW` with `PIPE_ACCESS_DUPLEX | FILE_FLAG_FIRST_PIPE_INSTANCE` on the first instance, `PIPE_REJECT_REMOTE_CLIENTS`, and a `SECURITY_ATTRIBUTES` from `ConvertStringSecurityDescriptorToSecurityDescriptorW` with `D:P(A;;GA;;;<current user SID>)`. The default DACL grants Everyone read, so it must not be used.
- Peer: `GetNamedPipeClientProcessId`, then `QueryFullProcessImageNameW`.
- `client_gone`: `PeekNamedPipe` returns `ERROR_BROKEN_PIPE`.
- Read timeout: a watchdog calls `CancelIoEx` if no full request arrives within `READ_TIMEOUT`.

- [ ] **Step 3: Implement the Windows CLI connect** in `onepass-cli/src/main.rs`

Open the pipe with `std::fs::OpenOptions`, then verify with `GetSecurityInfo` that the pipe's owner SID equals the current user's, because the pipe namespace is global and another user could squat the name. Mismatch: `Error::Io("pipe is not owned by the current user")`.

- [ ] **Step 4: Verify the compile**

Run: `rustup target add x86_64-pc-windows-gnu && cargo check -p onepass-cli --target x86_64-pc-windows-gnu`
Expected: success. Also run `cargo test -p onepass-cli` and `cargo test -p one-pass --test ipc` on Linux: still PASS.

`one-pass` may not cross-check from Linux because of its Windows build scripts. If it fails for that reason, record it. `transport/windows.rs` then first compiles on Windows during the manual checklist, and the final report must say so.

- [ ] **Step 5: Run the `security-reviewer` agent** on the Windows transport

- [ ] **Step 6: Commit**

`git commit -m "feat: add Windows named pipe transport"`

---

### Task 14: Docs and final verification

**Files:**
- Modify: `README.md`, `AGENTS.md`, `.env.example`, `src-tauri/tauri.conf.json`, `src-tauri/Cargo.toml`
- Create: `docs/manual-test-checklist.md`

- [ ] **Step 1: Rewrite `README.md`**

Cover: what it is, build and install including `cargo install --path onepass-cli`, the `op` name collision with 1Password (D11), `.env` usage, and approval modes. It must include the spec's known limit: "a process running as the same OS user can read the child's environment while it runs, and a legitimate command can print its own environment. This design removes the 'agent reads `.env`' path, not a fully hostile same-user process."

- [ ] **Step 2: Clean up leftovers**

- `AGENTS.md`: remove the stale rule saying CSP and `pnpm` are leftovers to fix, since they are already fixed. Update the Architecture section for the new modules.
- `src-tauri/Cargo.toml`: real `description` and `authors`.
- `tauri.conf.json`: `minWidth`/`minHeight` for the three-pane layout. CSP is unchanged.
- Resolve remaining `shortcut:` comments in the CLI about Windows: `grep -rn "shortcut:" onepass-* src-tauri/src`.

- [ ] **Step 3: Write `docs/manual-test-checklist.md`**

One section each for Linux, macOS and Windows. Each covers:
- First run, unlock, lock, auto-lock after sleep.
- Dialog raises the window while another app is focused; Ctrl-C closes the dialog.
- Reveal re-hides at 10 s; clipboard clears at 30 s.
- `op run` end to end for allow, deny and locked.
- Socket or pipe permissions: `ls -l` on Unix; `accesschk` from another user on Windows.
- Windows: pipe owner check, `transport/windows.rs` compiles.

- [ ] **Step 4: Verify everything**

Run: `cargo fmt --all --check && cargo test --workspace && npm test && npm run build && npm run tauri build -- --debug --no-bundle`
Expected: all succeed.

- [ ] **Step 5: End-to-end smoke test on Linux**

Run `npm run tauri dev`, create a vault, and add `demo/dev/TEST_KEY`. From the repo root run `cargo run -p onepass-cli --bin op -- run --env-file .env.example -- sh -c 'echo "$PLAIN ${#TEST_KEY}"'`.
Expected: the dialog appears, and after Allow the output is `literal 14` for a 14-character value. WSL needs WSLg for the window. If no display is available, stop and ask, per `AGENTS.md`.

- [ ] **Step 6: Run `security-reviewer` and `rust-reviewer` on the whole branch**

- [ ] **Step 7: Commit**

`git commit -m "docs: add README, manual test checklist and final cleanup"`
