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

## Linux

- [ ] `ls -l $XDG_RUNTIME_DIR/onepass.sock` shows `srw-------` and your user.
- [ ] Dialog shows the calling PID and executable of `op`.

## macOS

- [ ] Socket is under `$TMPDIR` with mode 0600.
- [ ] Dialog shows the PID and executable path (this code path is not compiled on the Linux dev host).

## Windows (transport not implemented yet)

- [ ] `transport/windows.rs` compiles on Windows.
- [ ] Pipe `\\.\pipe\onepass-<USERNAME>` is not accessible from another user (`accesschk`).
- [ ] `op` refuses a pipe not owned by the current user.
