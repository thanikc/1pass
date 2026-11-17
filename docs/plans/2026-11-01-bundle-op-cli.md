# Ship `op` Inside the App Installer: Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** One install of the released bundle gives the user both the app and `op`, with `op` on `PATH` wherever the installer can arrange it.

**Architecture:** A bundle-only Tauri config adds `op` as an `externalBin` sidecar, built by a small Node script. `.deb`/`.rpm` place it in `/usr/bin`, and a Settings action covers macOS and AppImage, whose formats can't run install scripts. Windows is out of scope until its transport exists.

**Tech Stack:** Tauri 2 bundler, Rust (edition 2024), Angular 22, Node ESM.

**Spec:** `docs/superpowers/specs/2026-11-01-bundle-op-cli-design.md` (the "Amendments from planning" section overrides its Design section)

## Global Constraints

- `op` must never link crypto: build it with `-p onepass-cli` only, never `--workspace`, which would unify `onepass-core` features.
- `externalBin` stays out of `src-tauri/tauri.conf.json`. `cargo check`, `cargo test` and `tauri dev` must keep working with no `src-tauri/binaries/`.
- `invoke()` only in `src/app/api/vault-api.ts`. Commands are thin and logic stays Tauri-free.
- No new dependencies, no new capabilities.
- macOS destination is `/usr/local/bin/op`; AppImage destination is `~/.local/bin/op`.
- Never replace an `op` we didn't install.
- Commit messages end with `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.
- `cargo` isn't on the agent shell's `PATH`. The user approved using it from `~/.cargo/bin`, so prefix commands that need it with `PATH="$HOME/.cargo/bin:$PATH"`. If any other tool isn't available, stop and ask (AGENTS.md).

## Review Focus

1. App launched from the mounted `.dmg` or a translocated path: refuse, rather than link to a path that disappears. Test: `ephemeral_paths` (Task 2).
2. A 1Password `op` already at the destination, as a file or a symlink: left untouched, with a clear error. Tests: `classify_*`, `refuses_foreign_and_leaves_it` (Task 2).
3. A dangling symlink at `~/.local/bin/op`: replaced, without writing through to the link's target. Test: `copy_replaces_dangling_link_without_following` (Task 2).
4. macOS app location containing spaces or quotes: passed as data, never spliced into a script. Check: manual checklist (Task 2).
5. A 1Password CLI package that owns `/usr/bin/op`: the `.deb`/`.rpm` install may fail. The user accepted this; the README must say so (Task 1).

---

### Task 1: Bundle `op` in the installers

**Files:**
- Create: `scripts/build-op.mjs`
- Create: `src-tauri/tauri.bundle.conf.json`
- Modify: `package.json` (scripts)
- Modify: `.gitignore`
- Modify: `README.md` (Build and run, Install `op`)

**Interfaces:**
- Produces: `npm run bundle`, which writes `src-tauri/binaries/op-<triple>` and builds installers containing `op`. Accepts extra args, e.g. `npm run bundle -- --bundles deb`.

- [ ] **Step 1: Write `scripts/build-op.mjs`**

Read the triple from `TAURI_ENV_TARGET_TRIPLE` and exit non-zero with a message if unset. Run `cargo build --release -p onepass-cli --target <triple>` with `spawnSync` and `stdio: "inherit"`, and exit non-zero on failure. Copy `target/<triple>/release/op` to `src-tauri/binaries/op-<triple>`, creating the directory. Resolve paths from `import.meta.url`.

- [ ] **Step 2: Add `src-tauri/tauri.bundle.conf.json`**

```json
{
  "build": { "beforeBuildCommand": "npm run build && node scripts/build-op.mjs" },
  "bundle": {
    "targets": ["deb", "rpm", "appimage", "app", "dmg"],
    "externalBin": ["binaries/op"]
  }
}
```

- [ ] **Step 3: Wire it up**

Add the script `"bundle": "tauri build --config src-tauri/tauri.bundle.conf.json"` to `package.json`, and `/src-tauri/binaries` to `.gitignore`.

- [ ] **Step 4: Verify the normal build doesn't need `op`**

Run: `rm -rf src-tauri/binaries && cargo check --manifest-path src-tauri/Cargo.toml`
Expected: finishes with no error.

- [ ] **Step 5: Build a `.deb` and check its contents**

Run: `npm run bundle -- --bundles deb && dpkg-deb -c target/release/bundle/deb/*.deb | grep -E 'usr/bin/(op|one-pass)$'`
Expected: both `./usr/bin/op` and `./usr/bin/one-pass` are listed. Also confirms that listing non-Linux targets doesn't fail the build; if it does, stop and report.

- [ ] **Step 6: Verify `op` has no crypto and runs**

Run: `cargo tree -p onepass-cli -e normal | grep -iE 'argon2|chacha'`
Expected: no output.
Run: `src-tauri/binaries/op-x86_64-unknown-linux-gnu`
Expected: the usage text.

- [ ] **Step 7: Try the other Linux formats**

Run: `npm run bundle -- --bundles rpm,appimage`
Expected: both are produced. AppImage tooling may need FUSE or a download under WSL; if it fails for that reason, report it rather than working around it. Don't commit any changes to `src-tauri/gen/`.

- [ ] **Step 8: Update the README**

"Build and run": `npm run bundle` builds installers that include `op`; plain `npm run tauri build` does not. "Install `op`": installers include it. `.deb`/`.rpm` put it in `/usr/bin`, macOS and AppImage use Settings → Command-line tool. Keep `cargo install --path onepass-cli` for source builds. Keep the 1Password name warning and add that a `.deb`/`.rpm` install may fail if a 1Password CLI package already owns `/usr/bin/op`.

- [ ] **Step 9: Commit**

```bash
git add scripts/build-op.mjs src-tauri/tauri.bundle.conf.json package.json .gitignore README.md
git commit -m "build: include op in the app installers"
```

### Task 2: Install logic and commands

**Files:**
- Create: `src-tauri/src/cli_install.rs` (with its `#[cfg(test)]` module)
- Modify: `src-tauri/src/lib.rs` (`pub mod cli_install;`, register two commands)
- Modify: `src-tauri/src/commands.rs`
- Modify: `src/app/api/vault-api.ts`
- Modify: `docs/manual-test-checklist.md`

**Interfaces:**
- Produces, in `cli_install.rs`:
  - `pub fn target_for(os: &str, appimage: Option<&OsStr>, home: Option<&Path>) -> Option<PathBuf>`
  - `pub fn ephemeral(exe: &Path) -> bool`
  - `pub enum Existing { Absent, Ours, Foreign }` deriving `Debug, PartialEq`
  - `pub fn classify(dest: &Path, source: &Path) -> Result<Existing, Error>`
  - `pub fn install(exe: &Path, dest: &Path) -> Result<(), Error>`
- Produces, Tauri commands: `cli_install_target() -> R<Option<String>>` and `install_cli() -> R<String>` (the installed path). Neither takes arguments from the webview.
- Produces, `VaultApi`: `cliInstallTarget = () => invoke<string | null>("cli_install_target")` and `installCli = () => invoke<string>("install_cli")`.

- [ ] **Step 1: Write the failing tests**

Use temp dirs as `session.rs` tests do (`std::env::temp_dir()` with a tag and the PID). Gate filesystem tests with `#[cfg(all(test, unix))]`.

```rust
#[test]
fn targets() {
    let home = Path::new("/home/u");
    let img = Some(OsStr::new("/x/one-pass.AppImage"));
    assert_eq!(target_for("macos", None, Some(home)), Some("/usr/local/bin/op".into()));
    assert_eq!(target_for("linux", img, Some(home)), Some("/home/u/.local/bin/op".into()));
    assert_eq!(target_for("linux", None, Some(home)), None); // .deb/.rpm
    assert_eq!(target_for("linux", img, None), None);
    assert_eq!(target_for("windows", None, Some(home)), None);
}

#[test]
fn ephemeral_paths() {
    assert!(ephemeral(Path::new("/Volumes/one-pass/one-pass.app/Contents/MacOS/one-pass")));
    assert!(ephemeral(Path::new("/private/var/folders/a/AppTranslocation/b/d/one-pass.app/Contents/MacOS/one-pass")));
    assert!(!ephemeral(Path::new("/Applications/one-pass.app/Contents/MacOS/one-pass")));
}
```

Filesystem tests, with `source` an existing file containing `b"ours"`:
- `classify_absent`: a missing dest, and a dangling symlink, both give `Absent`.
- `classify_ours`: a symlink to `source`, and a regular file with identical bytes, both give `Ours`.
- `classify_foreign`: a regular file with different bytes, and a symlink to a different existing file, both give `Foreign`.
- `copy_creates_parent_and_is_executable`: `install` with `exe` beside `op`, dest in a missing dir. Dest bytes equal the source and `mode & 0o777 == 0o755`. Gate with `target_os = "linux"`.
- `copy_replaces_dangling_link_without_following`: dest is a symlink to a missing path. After `install`, dest is a regular file and the link's target still doesn't exist. Gate with `target_os = "linux"`.
- `refuses_foreign_and_leaves_it`: dest holds other bytes. `install` returns `Err` and dest bytes are unchanged.
- `missing_bundled_op_errors`: `exe` has no sibling `op`. `install` returns `Err`.

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib cli_install`
Expected: compile failure, `cli_install` not found.

- [ ] **Step 3: Implement `src-tauri/src/cli_install.rs`**

- `target_for`: as pinned by the test. It returns `None` wherever the installer already handled `PATH`.
- `ephemeral`: path starts with `/Volumes/` or contains an `AppTranslocation` component.
- `classify`: `symlink_metadata`; not found gives `Absent`. A symlink whose `read_link` equals `source` gives `Ours`. If `dest.exists()` is false (dangling) gives `Absent`. Identical bytes give `Ours`. Anything else gives `Foreign`.
- `install`: if `cfg!(target_os = "macos") && ephemeral(exe)`, return `Err` telling the user to move the app to Applications first. Source is `exe`'s sibling `op` (`std::env::consts::EXE_SUFFIX`), canonicalized; if missing, return `Err`. `classify`: `Foreign` returns `Err` with `"{dest} already exists and isn't this app's op. Remove it and try again."`, `Ours` returns `Ok`, `Absent` places it. Use `Error::Io` as `commands.rs` does.
- Placing on Linux: `create_dir_all` the parent, write to `op.tmp-<pid>` in that directory using `OpenOptions` with `create_new(true)` and `mode(0o755)`, then `rename` over dest. Remove the temp file on failure. Add `// shortcut: an older copy from a previous AppImage counts as Foreign, so upgrades need a manual delete; upgrade if AppImage users hit this.`
- Placing on macOS: run this, passing the path as an argument so it's never spliced into the script. A non-zero exit returns `Err` with stderr (this includes a cancelled admin prompt).

```rust
Command::new("/usr/bin/osascript")
    .args(["-e", "on run argv"])
    .args(["-e", r#"do shell script "mkdir -p /usr/local/bin && ln -sfn " & quoted form of item 1 of argv & " /usr/local/bin/op" with administrator privileges"#])
    .args(["-e", "end run", "--"])
    .arg(&source)
```

- On Windows: return `Err` so the crate still compiles. `target_for` returns `None` there, so the UI never offers this.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib cli_install`
Expected: all pass.

- [ ] **Step 5: Add the commands and API wrappers**

`cli_install_target` calls `target_for(std::env::consts::OS, std::env::var_os("APPIMAGE").as_deref(), std::env::home_dir().as_deref())`, in `commands.rs`. `install_cli` resolves the same target (`Err` if `None`), calls `install` with `std::env::current_exe()?`, and returns the path as a string. Register both in `generate_handler!` in `lib.rs`. Add the `VaultApi` wrappers from Interfaces.

- [ ] **Step 6: Check it builds**

Run: `cargo fmt --manifest-path src-tauri/Cargo.toml && cargo test --manifest-path src-tauri/Cargo.toml`
Expected: all pass.

- [ ] **Step 7: Add to the manual checklist**

Linux, AppImage: Settings action copies `op` to `~/.local/bin`, and `op run -- true` works after restarting the app. macOS: run from `/Applications` (admin prompt, then `ls -l /usr/local/bin/op`); cancel the prompt (an error shows, nothing changes); run from the mounted `.dmg` (refused); run from a folder with spaces and a quote in its name (link is correct); with Homebrew `op` at `/usr/local/bin/op` (refused, left intact).

- [ ] **Step 8: Commit**

```bash
git add src-tauri/src/cli_install.rs src-tauri/src/lib.rs src-tauri/src/commands.rs src/app/api/vault-api.ts docs/manual-test-checklist.md
git commit -m "feat(app): install the bundled op on macOS and AppImage"
```

### Task 3: Settings action

**Files:**
- Modify: `src/app/screens/settings/settings.component.ts`, `.html`
- Test: `src/app/screens/settings/settings.component.spec.ts`

**Interfaces:**
- Consumes: `VaultApi.cliInstallTarget()`, `VaultApi.installCli()` from Task 2.

- [ ] **Step 1: Write the failing tests**

Add `cliInstallTarget` and `installCli` to the `setup()` fake, with `cliInstallTarget` defaulting to resolve `null` and overridable by the test.

- `it("hides the CLI action when the installer already put op on PATH")`: target `null`, so `el.querySelector("[name=install-cli]")` is `null`.
- `it("shows the target, installs and reports the path")`: target `"/usr/local/bin/op"`, so the text contains it. After clicking `[name=install-cli]` and `tick()`, `installCli` was called once and the text contains `"Installed to /usr/local/bin/op"`.
- `it("shows why the install was refused")`: `installCli` rejects with `"already exists"`, so `[role=alert]` contains it and the password form's state is untouched.

- [ ] **Step 2: Run tests to verify they fail**

Run: `npm test -- --include src/app/screens/settings/settings.component.spec.ts`
Expected: the new tests fail, existing ones pass.

- [ ] **Step 3: Implement**

Add signals `cliTarget` (`string | null`), `cliError` and `cliNotice`, kept separate from the password form's `error` and `notice`. Load the target in `ngOnInit` in its own `try`, so a failure doesn't mask settings loading. `installCli()` clears both messages, awaits the API, and sets the notice to `` `Installed to ${path}. Open a new terminal to use it.` ``, or sets `cliError` via `errorMessage(e)`.

In the template, after the auto-lock label and only when `cliTarget()` is set: `<h2>Command-line tool</h2>`, a muted line naming the target, `<button type="button" name="install-cli">Install command-line tool</button>`, then the same `role="alert"` error and muted notice patterns the form uses.

- [ ] **Step 4: Run tests to verify they pass**

Run: `npm test`
Expected: all pass.

- [ ] **Step 5: Commit**

```bash
git add src/app/screens/settings/
git commit -m "feat(ui): add install command-line tool action to settings"
```

### Task 4: Final review

- [ ] **Step 1: Run the `security-reviewer` agent** on the branch (new IPC commands, privileged `osascript` call, config changes) and resolve its findings.

- [ ] **Step 2: Run everything**

Run: `cargo test --workspace && npm test`
Expected: all pass, with `src-tauri/binaries/` absent (`rm -rf` it first).
