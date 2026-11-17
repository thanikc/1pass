# one-pass: developer guide

How to build, test, bundle and release one-pass. For installing and using the app, see [README.md](README.md).

Tauri 2 (Rust, edition 2024) backend, Angular 22 + TypeScript 6 frontend. Linux and macOS are implemented. The Windows named-pipe transport is not written yet.

## Prerequisites

- Node.js and npm
- Rust stable (edition 2024)
- Linux only, the Tauri system libraries:

  ```bash
  sudo apt install pkg-config libdbus-1-dev libwebkit2gtk-4.1-dev libxdo-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev
  ```

## Build and run

```bash
npm install
npm run tauri dev          # desktop app with hot reload
npm run tauri build        # release build (app only, no `op`)
npm run bundle             # installers that include `op`
cargo test --workspace     # Rust tests
npm test                   # Angular tests
```

To install `op` from source without bundling:

```bash
cargo install --path onepass-cli
```

## Bundle and install a local build

```bash
npm run bundle
```

Packages are built into `target/release/bundle/` (`deb`, `rpm`, `appimage` on Linux; `dmg` on macOS). `npm run bundle` uses `src-tauri/tauri.bundle.conf.json`, which runs `scripts/build-op.mjs` to build `op` for the target and ship it as a sidecar.

- **Debian / Ubuntu (.deb)**:
  ```bash
  sudo apt install ./target/release/bundle/deb/one-pass_*.deb
  ```
- **Fedora / RHEL (.rpm)**:
  ```bash
  sudo rpm -i target/release/bundle/rpm/one-pass-*.rpm
  ```
- **AppImage**:
  ```bash
  chmod +x target/release/bundle/appimage/one-pass_*.AppImage
  ./target/release/bundle/appimage/one-pass_*.AppImage
  ```

## Releases

Pushing a tag `vX.Y.Z` runs `.github/workflows/release.yml`. It bundles Linux (`deb`, `rpm`, `AppImage`) and macOS (`dmg`, Apple Silicon only) installers stamped with version `X.Y.Z`, plus `SHA256SUMS-*.txt`, and attaches them to a draft GitHub Release for that tag. Review the draft and publish it by hand.

```bash
git tag v0.2.0 && git push origin v0.2.0
```

A pre-release tag such as `v0.3.0-rc.1` creates a release marked as pre-release and skips the `.rpm`, because RPM versions can't contain `-`. The version comes from the tag only; the `0.1.0` in `tauri.conf.json`, `Cargo.toml` and `package.json` is not changed. macOS builds are not signed or notarized.

## Project layout

- `onepass-core`: vault format (Argon2id + XChaCha20-Poly1305), data model, wire protocol.
- `onepass-cli`: the `op` binary. Never links crypto.
- `src-tauri`: the app: session, approval broker, local socket, Tauri commands.
- `src`: the Angular UI.
- `docs/manual-test-checklist.md`: what to check by hand on each OS.
- `AGENTS.md`: architecture notes and rules for contributors and coding agents.
