# Ship `op` inside the app installer

## Intent
The app and the `op` CLI are useless apart, but today `op` is installed separately
(`cargo install --path onepass-cli`). One install of the released bundle should give
the user both, with `op` on `PATH`.

## Goals
- Installing the app on Linux, macOS or Windows also installs `op`.
- `op` is on `PATH` after install wherever the installer can arrange it.

## Non-goals
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
- Name clash with the 1Password CLI: no package conflict declared (matches README). *Assistant's call.*
- Symlink locations as above: confirmed with user.

## Left to the builder
- Exact build script (shell vs. cargo `build.rs`), cross-platform target-triple detection.
- Exact UI placement of the install action.
- Verify the NSIS PATH edit handles per-user vs per-machine installs.
- Linux/macOS/Windows testing is manual (no CI); extend the manual test checklist.
