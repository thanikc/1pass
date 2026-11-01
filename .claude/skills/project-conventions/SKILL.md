---
name: project-conventions
description: Conventions for this Tauri + Angular app: IPC patterns, Angular style, and secrets handling. Background knowledge for any code change.
user-invocable: false
---

# Conventions

## IPC
- Rust: `#[tauri::command] fn name(args) -> Result<T, String-or-error-enum>`; register in `generate_handler!` in `src-tauri/src/lib.rs`.
- Angular: wrap each `invoke<T>("name", {...})` in a typed method of an injectable service under `src/app/`; components never call `invoke` directly (the template's `greet` demo is the exception to clean up).
- Tauri converts camelCase JS argument keys to snake_case Rust params.

## Angular
- Standalone components, `signal()` for state, `inject()` for DI, new control-flow syntax (`@if`, `@for`).
- Strict TypeScript; no `any`.

## Secrets
- Secrets are handled in Rust, not the frontend. The frontend receives only what it must display.
- Never log secrets; zeroize buffers; no plaintext persistence.
- Any change to `tauri.conf.json` security settings or `capabilities/` should be reviewed with the `security-reviewer` agent.
