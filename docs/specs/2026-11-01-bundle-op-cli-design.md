# Ship `op` inside the app installer

## Intent
The app and the `op` CLI are useless apart, but today `op` is installed separately
(`cargo install --path onepass-cli`). One install of the released bundle should give
the user both, with `op` on `PATH`.

## Goals
- Installing the app on Linux or macOS also installs `op`.
- `op` is on `PATH` after install wherever the installer can arrange it.

## Non-goals
- Windows, until its transport exists (see Amendments).
- A release/CI workflow, signing, or auto-update (none exist yet).
- A standalone `install.sh`; the installers are the install path.
- Changing `op` or the wire protocol.

## Anti-goals
- `op` must still never link crypto (see AGENTS.md).
- No silent privileged writes outside the installer or an explicit user action.

## Approaches considered
- **A. Sidecar + per-installer PATH handling (chosen):** automatic where possible.
- B. Sidecar + one in-app "Install to PATH" action everywhere: simpler, but extra
  manual step on Linux/Windows.
- C. macOS `.pkg` with postinstall: Tauri can't produce it; rejected.

## Design
1. **Bundle:** add `bundle.externalBin: ["binaries/op"]` in `src-tauri/tauri.conf.json`.
   Tauri needs `src-tauri/binaries/op-<target-triple>` at build time and strips the
   suffix when bundling. Extend `beforeBuildCommand` (script) to
   `cargo build --release -p onepass-cli` and copy the result there. The
   `binaries/` dir is gitignored.
2. **Linux `.deb`/`.rpm`:** `op` lands next to the app binary in `/usr/bin`. No extra work.
3. **Windows NSIS:** `bundle.windows.nsis.installerHooks` -> `src-tauri/windows/hooks.nsh`;
   post-install appends the install dir to `PATH`, post-uninstall removes it.
4. **macOS `.dmg` and Linux AppImage:** cannot run install scripts. Add an in-app
   "Install command-line tool" action (Tauri command in `commands.rs`, registered in
   `lib.rs`, wrapper in `src/app/api/vault-api.ts`, minimal capability) that symlinks
   the bundled `op`: `/usr/local/bin/op` on macOS (admin prompt), `~/.local/bin/op`
   on AppImage. Refuse to overwrite an existing non-symlink `op`; report clearly.
5. **Docs:** update README "Install `op`" to describe the bundled install.

## Decisions
- Name clash with the 1Password CLI: no package conflict declared, and `op` keeps its name. Confirmed with user, including that a `.deb`/`.rpm` install may fail next to a 1Password CLI package that owns `/usr/bin/op`: one-pass is meant as a free alternative to 1Password.
- Symlink locations as above: confirmed with user.

## Amendments from planning
These override the Design section above where they differ.
- **Windows is dropped for now (user decision), so Design item 3 is out.** `op` on Windows
  can't reach the app: `connect()` in `onepass-cli` and `start_ipc` in `src-tauri` both
  return "not supported yet". Add the `PATH` hook when the Windows transport is written.
- **`externalBin` moves to `src-tauri/tauri.bundle.conf.json`, used by a new `npm run bundle`.**
  `tauri-build` copies external binaries on every cargo build (`copy_binaries` in
  tauri-build 2.7.4), so listing it in `tauri.conf.json` would tie `cargo check`,
  `cargo test` and `tauri dev` to a prebuilt `op`.
- **AppImage copies `op` instead of symlinking.** The AppImage mount path changes on every
  run, so a symlink into it would dangle.
- **An existing symlink is replaced only if it is dangling.** Replacing any symlink would
  clobber a Homebrew-installed 1Password `op` at `/usr/local/bin/op`.
- **The action refuses to run from a mounted `.dmg` or a translocated app.** A link to
  either path would break.
- **No capability change.** `build.rs` declares no app manifest, so app commands aren't
  gated by capabilities.
- **Installers target `deb`, `rpm`, `appimage`, `app`, `dmg`.**

## Left to the builder
- Exact build script (shell vs. cargo `build.rs`), cross-platform target-triple detection.
- Exact UI placement of the install action.
- Linux/macOS testing is manual (no CI); extend the manual test checklist.
