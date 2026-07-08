# Feature — Error Handling

## Status (2026-07)

Mostly implemented: capture degrades every failure to a warning and always
saves; restore reports failed_items / warnings / closed_items honestly in the
report modal. Remaining work keeping this spec in pending/:
- "log errors" — no logging framework or log file exists yet
- "allow manual recall" — no way to retry only the failed items of a restore

## Goal

Handle incomplete capture and restore gracefully.

---

## Capture Errors

- missing command line
- unknown process
- inaccessible window

---

## Restore Errors

- app failed to launch
- window not found
- layout mismatch

---

## Behavior

- never crash
- always save snapshot
- log errors
- show warnings to user

---

## User Feedback

- show list of issues
- allow manual recall