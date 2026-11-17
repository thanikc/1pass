# one-pass

A desktop password manager that keeps secrets out of `.env` files. The app holds the encrypted vault. Your `.env` holds references like `op://project/environment/NAME`. The `op run` command asks the app for the values, the app asks you, and only then does the command start with the secrets in its environment.

## Install

Download the installer for your system from the [Releases page](https://github.com/thanikc/1pass/releases). Linux and macOS are supported; Windows is not yet.

- **Debian / Ubuntu (.deb)**:
  ```bash
  sudo apt install ./one-pass_*_amd64.deb
  ```
- **Fedora / RHEL (.rpm)**:
  ```bash
  sudo rpm -i one-pass-*.x86_64.rpm
  ```
- **AppImage**:
  ```bash
  chmod +x one-pass_*.AppImage
  ./one-pass_*.AppImage
  ```
- **macOS (.dmg)**: Apple Silicon only (Intel Macs are not supported). Open the `.dmg` and drag one-pass to Applications. The app is not signed yet, so macOS blocks the first launch: right-click the app, choose Open, and confirm.

Each release has `SHA256SUMS-*.txt` files to check your download, e.g. `sha256sum -c SHA256SUMS-x86_64-unknown-linux-gnu.txt --ignore-missing`.

## Run

- **App**: launch **one-pass** from your application menu, or run `one-pass` in a terminal.
- **CLI**: check `op` with `op --help`.

## Install `op`

The `.deb` and `.rpm` packages install `op` to `/usr/bin`. On macOS and AppImage, use Settings → Command-line tool in the app to install it (`/usr/local/bin/op` on macOS, `~/.local/bin/op` for AppImage).

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

To move an existing `.env` into the vault, select an environment and use "Import .env". Pick the file, then tick the variables to store. Names and status are shown, never values. Secret-looking literals are preselected; references and empty values can't be imported. An existing entry is replaced only when ticked (its notes are kept), and the import is all-or-nothing. The `.env` file is never modified: the app then shows `NAME=op://project/environment/NAME` lines with a Copy button to paste into it. Files over 1 MiB or not UTF-8 are refused.

In the entry list, the search field matches name, notes, project or environment across all environments, never values. In the entry editor, Generate fills in 32 random letters and digits. Values can be multi-line (for example PEM keys); trailing line breaks are stripped on save. Shortcuts: Ctrl/Cmd+F searches, Ctrl/Cmd+L locks (while unlocked).

### Approval modes

Chosen per project in the dialog and changeable in the Approvals tab. Changing a project's mode revokes its approvals.

| Mode | Behaviour |
|------|-----------|
| Ask every run | Prompt each time |
| Time-boxed | Reuse the approval for 15 minutes, 1 hour or until the vault locks |
| Remember | Stored inside the encrypted vault, survives lock and restart |

An approval covers the same project directory, the same executable name and the same set of references. A different reference set or executable asks again. Three denials (or unanswered prompts) for a project within a minute block further requests for it for a minute.

The vault auto-locks after 15 minutes idle by default (Settings). Copied values are cleared from the clipboard after 30 s. Revealed values hide again after 10 s.

### Closing the window

Closing the window hides it and the app keeps running in the tray, so `op run` still works. Hiding does not lock the vault; auto-lock still applies (with auto-lock set to never, it stays unlocked until Lock or Quit). The tray menu has Open, Lock and Quit. A left click on the tray icon opens the window, but Tauri doesn't deliver tray click events on Linux, so use the menu's Open there. An `op run` request or starting one-pass again also brings the window back. If no tray icon can be created, closing the window quits.

Stock GNOME shows no tray icons without an AppIndicator extension. Without a visible tray, start one-pass again to show the window.

## Known limit

A process running as the same OS user can read the child's environment while it runs, and a legitimate command can print its own environment. This design removes the "agent reads `.env`" path, not a fully hostile same-user process.

## Development

To build from source, run the tests or cut a release, see [DEV.md](DEV.md).
