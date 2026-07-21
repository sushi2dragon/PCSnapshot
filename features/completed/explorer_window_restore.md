# File Explorer window capture and restore

## Behavior

- Capture every visible File Explorer folder window with its filesystem/UNC path or supported virtual root, restored geometry, state, and monitor.
- Show File Explorer in snapshot contents with the number of captured folder windows.
- Reload the selected snapshot details after recapture so that count reflects the newly persisted folders.
- Restore one window per saved location, reusing an already-open matching folder before opening a new window.
- Allow an additive File Explorer-only restore from the contents-row restore button.
- Start New closes every enumerated Explorer folder window through `WM_CLOSE`, confirms the HWNDs disappear, and never terminates the shared Windows shell process.
- On clean restore, close extra captured-type Explorer folder windows and verify each close against ShellWindows.
- Keep `explorer.exe` in the system-protected process list. The app never launches, closes, or terminates the Windows shell process itself.

## Supported locations

- Filesystem folders, including percent-encoded paths
- UNC shares
- This PC
- Home / Quick Access
- Recycle Bin

Search results, Control Panel pages, and unknown shell namespaces are skipped with a capture warning because they cannot be reconstructed reliably.

## Compatibility

Schema version 4 adds `explorer_windows`. Older snapshots deserialize with an empty list, and clean restore does not infer that an older snapshot intended to close Explorer folders.
