# Feature — Capture Engine

## Goal

Capture the current desktop environment and convert it into a structured snapshot.

---

## Trigger

User clicks "Take Snapshot"

---

## Capture Pipeline

1. Detect foreground window
2. Enumerate all visible windows
3. Map windows → processes
4. Collect process metadata
5. Extract command-line arguments
6. Capture layout (positions, monitors)
7. Send data to context extraction layer
8. Capture screenshot
9. Generate snapshot object

---

## Foreground Priority

The foreground app must be processed first because:
- it represents current intent
- highest probability of useful context

---

## Captured Data

### Process Info
- name
- PID
- executable path
- command line
- classification

### Window Info
- title
- position
- size
- state (minimized/maximized)
- monitor index

### System Info
- timestamp (implemented)
- display layout / resolution — NOT implemented; only per-window
  monitor_index is captured today. Revisit if multi-monitor restore
  fidelity needs it.

---

## Performance Requirements

- capture must complete quickly (<3s)
- avoid blocking UI
- run in background thread

---

## Output

Returns a raw snapshot object passed to:
- context extraction
- storage system