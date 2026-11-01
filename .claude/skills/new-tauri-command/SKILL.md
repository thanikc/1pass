---
name: new-tauri-command
description: Scaffold a new Tauri command end to end (Rust fn, registration, capability check, typed Angular service wrapper)
disable-model-invocation: true
argument-hint: <command_name> [description]
---

Create a new Tauri command named `$ARGUMENTS` (snake_case in Rust, camelCase in TypeScript).

1. In `src-tauri/src/lib.rs` (or a new module if it grows), add a `#[tauri::command]` fn returning `Result<T, E>` with input validation. No `unwrap`.
2. Register it in `tauri::generate_handler![...]`.
3. If it needs a plugin or non-default permission, update `src-tauri/capabilities/default.json` with the minimum permission.
4. Add a typed wrapper method to an injectable Angular service in `src/app/` using `invoke<T>("command_name", { ... })`.
5. Verify: `cargo check --manifest-path src-tauri/Cargo.toml` and `npm run build`.
6. If the command touches secrets, suggest running the `security-reviewer` agent.
