# Feature — Session Storage

## Goal

Store snapshots in a structured, readable format.

---

## Storage Format

JSON file per snapshot

---

## Structure

- id
- name
- timestamp
- processes
- windows
- context clues
- restore hints
- warnings
- thumbnail path

---

## File Layout

AppData/Snapshots/

- snap_<timestamp_ms>.json (e.g. snap_1751212345678.json)
- snap_<timestamp_ms>.png

IDs are timestamp-based, not sequential; the sequential "Snapshot NN" scheme
applies only to the default display name.

---

## Naming

Default:
- Snapshot 01
- Snapshot 02

User override allowed

---

## Requirements

- human-readable
- versioned schema
- tolerant to partial corruption