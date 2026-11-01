# one-pass

Password-manager desktop app. Tauri 2 (Rust, edition 2024) backend in `src-tauri/`, Angular 22 + TypeScript 6 frontend in `src/`. Treat everything as security-sensitive.

## Commands

- `npm start` — Angular dev server on port 1420 (set in `angular.json`)
- `npm run tauri dev` — full desktop app (runs `beforeDevCommand`)
- `npm run build` — Angular production build into `dist/one-pass/browser`
- `cargo check --manifest-path src-tauri/Cargo.toml` — type-check Rust
- `cargo fmt --manifest-path src-tauri/Cargo.toml`

## Architecture

- Frontend calls Rust via `invoke()` from `@tauri-apps/api/core`.
- Rust commands are `#[tauri::command]` fns in `src-tauri/src/lib.rs`, registered in `generate_handler![...]`.
- Window permissions live in `src-tauri/capabilities/default.json`; grant the minimum.
- Angular uses standalone components and signals (see `src/app/app.component.ts`).

## Rules

- Never log, persist in plaintext, or send secrets over IPC beyond what is needed.
- Don't hand-edit `package-lock.json`, `Cargo.lock` or `src-tauri/gen/` (a hook blocks it).
- `tauri.conf.json` has `"csp": null` and `beforeDevCommand`/`beforeBuildCommand` use `pnpm` although the repo has `package-lock.json` (npm). Both are template leftovers to fix.
- Angular 22, Tauri 2 and TS 6 are newer than much training data: check docs via the context7 MCP server before using unfamiliar APIs.
