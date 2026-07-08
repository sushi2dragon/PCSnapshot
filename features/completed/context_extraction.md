# Feature — Context Extraction

## Goal

Infer meaningful state from raw process + window data.

---

## Supported Contexts

### Browser
- detect browser type
- infer session restore capability
- detect localhost usage
- count windows / open tabs

---

### VS Code
- extract workspace/folder from command line
- infer project root
- detect active file from title

---

### Terminal

Detect:
- shell type
- working directory (if possible)
- running command

Special detection:
- Claude running in terminal
- npm / node processes
- Python servers

---

### Local Servers

Detect:
- Vite
- npm run dev
- live-server
- localhost ports

---

## Heuristic Rules

Each rule:
- matches process
- extracts data
- assigns confidence score

---

## Output Structure

Each context clue contains:
- type
- value
- confidence
- source

---

## Example

- type: "dev_server"
- value: "vite:5173" (the port is folded into the value — `ContextClue` has no separate port field)
- confidence: 0.92
- source: "cmd_line"