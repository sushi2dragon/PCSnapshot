# Feature — Macro Layer

## Goal

Improve restore quality using small automation steps.

---

## Use Cases

- restore browser tabs (as built: captured tab URLs are passed as launch
  arguments — more reliable than the originally planned Ctrl+Shift+T macro)
- restore terminal sessions (as built: a generated restore script sets the
  CWD and shows recent history, rather than replaying keystrokes)
- focus windows (foreground app is focused last so it ends on top)

---

## Rules

- short and targeted
- only used when needed
- never primary restore method

---

## Execution

- after app launch
- tied to app type

---

## Safety

- avoid destructive actions
- retry limited times