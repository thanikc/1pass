# Manual test checklist

Run each section on its OS before a release. Automated tests cover logic; these cover the window, the OS and the real socket.

## Every platform

- [ ] First run: create a vault (mismatched confirm and short password are refused), lock, unlock with a wrong then the right password.
- [ ] Auto-lock fires after the chosen idle time. Suspend the machine past the timeout: on wake the vault is locked and approvals are gone.
- [ ] With another app focused, run `op run -- true` against a locked vault: the window comes to the front on the unlock screen, then the approval dialog appears.
- [ ] Ctrl-C `op` while the dialog is open: the dialog disappears and nothing is granted.
- [ ] Allow is disabled for a moment after the dialog opens; Deny has focus.
- [ ] Reveal re-hides after 10 s. Copy clears the clipboard after 30 s unless you copied something else.
- [ ] `op run` end to end: allow (child sees the value), deny (`op` exits non-zero, child not started), vault locked and left alone (times out).
- [ ] Exit code and signal of the child are passed through.
- [ ] Starting a second instance raises the first.

## Background tray (every platform with a tray icon)

- [ ] Close the window: it hides, the app keeps running and `op run -- true` still reaches it.
- [ ] Tray Open (or left click on macOS) shows and focuses the hidden window. On Linux left click does nothing; use the menu.
- [ ] Tray Lock locks the vault, and time-boxed approvals are gone.
- [ ] Tray Quit exits the app, removes the socket and clears a copied value from the clipboard.
- [ ] Hide the window, then `op run -- true`: the window is shown and focused (unlock or approval dialog).
- [ ] Hide the window, then start one-pass again: the first instance's window is shown and focused.
- [ ] Auto-lock 1 minute, window hidden: the vault is locked after 1 minute.
- [ ] If the tray icon can't be created (no tray library installed), closing the window quits.
- [ ] Stock GNOME without an AppIndicator extension: closing hides the window; starting one-pass again shows it.

## Env file import

- [ ] Import .env, cancel the file dialog: nothing changes.
- [ ] A file with a syntax error, a file over 1 MiB and a non-UTF-8 file are each refused with an error, and nothing is stored.
- [ ] Checklist shows names and status only, never values. Secret-looking literals are preselected; references and empty values can't be ticked.
- [ ] An existing entry is replaced only when ticked, and its notes are kept.
- [ ] After importing, the `NAME=op://project/environment/NAME` lines are shown. Copy puts them on the clipboard, and `op run` with them resolves.
- [ ] The `.env` file is byte-for-byte unchanged.

## Linux

- [ ] `ls -l $XDG_RUNTIME_DIR/onepass.sock` shows `srw-------` and your user.
- [ ] Dialog shows the calling PID and executable of `op`.
- [ ] AppImage: Settings → Command-line tool copies `op` to `~/.local/bin`, and `op run -- true` works after restarting the app.

## macOS

- [ ] Socket is under `$TMPDIR` with mode 0600.
- [ ] Dialog shows the PID and executable path (this code path is not compiled on the Linux dev host).
- [ ] Settings → Command-line tool from `/Applications`: admin prompt, then `ls -l /usr/local/bin/op` shows the link.
- [ ] Cancel the admin prompt: an error shows and nothing changes.
- [ ] Run from the mounted `.dmg`: refused.
- [ ] Run from a folder with spaces and a quote in its name: the link is correct.
- [ ] With Homebrew `op` at `/usr/local/bin/op`: refused and left intact.

## Windows (transport not implemented yet)

- [ ] `transport/windows.rs` compiles on Windows.
- [ ] Pipe `\\.\pipe\onepass-<USERNAME>` is not accessible from another user (`accesschk`).
- [ ] `op` refuses a pipe not owned by the current user.
