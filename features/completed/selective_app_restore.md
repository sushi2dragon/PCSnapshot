# Feature — Selective app restore

## Goal

Restore one application from a saved snapshot without changing the rest of the
current desktop.

## Trigger

Hover or keyboard-focus an application in a snapshot's Contents list and use
its compact restore button.

## Behavior

- Restores every captured process and window belonging to the selected executable.
- Reuses the normal restore engine for launch, context recovery, positioning, and warnings.
- Brings the selected application to the foreground when restoration finishes.
- Never closes other applications or surplus windows.
- Does not mark the full snapshot as the active session.
- Rejects missing executable paths and executables that are not in the snapshot.

## Context

The transient restore slice retains only context needed by the selected app:
terminal sessions, browser sessions, browser tab hints, Office file hints, and
VS Code workspace/folder hints.
