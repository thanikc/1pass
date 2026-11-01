# one-pass

A desktop password manager that keeps secrets out of `.env` files. The app holds the encrypted vault. Your `.env` holds references like `op://project/environment/NAME`. The `op run` command asks the app for the values, the app asks you, and only then does the command start with the secrets in its environment.

Tauri 2 (Rust) backend, Angular 22 frontend. Linux and macOS are implemented. The Windows named-pipe transport is not written yet.

## Build and run

```bash
npm install
npm run tauri dev          # desktop app with hot reload
npm run tauri build        # release build (app only, no `op`)
npm run bundle             # installers that include `op`
cargo test --workspace     # Rust tests
npm test                   # Angular tests
```

Linux needs the Tauri system libraries: `pkg-config libdbus-1-dev libwebkit2gtk-4.1-dev libxdo-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev`.

## Install `op`

The installers from `npm run bundle` include `op`. A `.deb` or `.rpm` puts it in `/usr/bin`. On macOS and AppImage, use Settings → Command-line tool to install it (`/usr/local/bin/op` on macOS, `~/.local/bin/op` for AppImage). For source builds:

```bash
cargo install --path onepass-cli
```

`op` **has the same name as the 1Password CLI.** If both are installed, whichever comes first on `PATH` wins; rename or alias one of them. A `.deb`/`.rpm` install may also fail if a 1Password CLI package already owns `/usr/bin/op`.

## Use

1. Start the app, create a vault with a master password and add entries (project / environment / name).
2. Put references in `.env` (use "Copy Reference" in the app):

   ```
   DATABASE_URL=op://myapp/dev/DATABASE_URL
   LOG_LEVEL=debug
   ```

3. Run your command through `op`:

   ```bash
   op run -- npm start
   op run --env-file .env.local -- node server.js
   ```

The app must be running. If it is locked it comes to the front for you to unlock. A dialog then shows the project directory, the command, the calling process and the variable names (never values). Exit codes and signals of the command are passed through.

### Approval modes

Chosen per project in the dialog and changeable in the Approvals tab. Changing a project's mode revokes its approvals.

| Mode | Behaviour |
|------|-----------|
| Ask every run | Prompt each time |
| Time-boxed | Reuse the approval for 15 minutes, 1 hour or until the vault locks |
| Remember | Stored inside the encrypted vault, survives lock and restart |

An approval covers the same project directory, the same executable name and the same set of references. A different reference set or executable asks again. Three denials (or unanswered prompts) for a project within a minute block further requests for it for a minute.

The vault auto-locks after 15 minutes idle by default (Settings). Copied values are cleared from the clipboard after 30 s. Revealed values hide again after 10 s. Closing the window quits the app.

## Known limit

A process running as the same OS user can read the child's environment while it runs, and a legitimate command can print its own environment. This design removes the "agent reads `.env`" path, not a fully hostile same-user process.

## Files

- `onepass-core`: vault format (Argon2id + XChaCha20-Poly1305), data model, wire protocol.
- `onepass-cli`: the `op` binary.
- `src-tauri`: the app: session, approval broker, local socket, Tauri commands.
- `src`: the Angular UI.
- `docs/manual-test-checklist.md`: what to check by hand on each OS.
