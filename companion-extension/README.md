# PC Snapshot Companion

The PC Snapshot Companion is a WebExtension that reports structured browser
state to the desktop app through native messaging. It is deliberately local:
it has no network access, content scripts, cookie access, or history access.

## What this first slice does

- Captures normal (non-private) browser windows and their bounds.
- Captures every tab's URL, title, order, active/pinned/muted/discarded state,
  and a snapshot-local tab-group key.
- Captures group title, color, collapsed state, and order when the browser
  supports the tab-groups API.
- Opens one persistent native-messaging port and responds to a
  `capture_request` from the desktop bridge.

It intentionally does **not** restore or close any browser tabs yet.

## Builds

`manifest.chromium.json` is for Chromium-family browsers. Chrome, Edge,
Opera, Brave, and Vivaldi need their own published/sideloaded package IDs.
`manifest.firefox.json` is for Firefox and has its own fixed add-on ID.

The desktop installer must replace the extension-ID placeholders in the native
host manifests and register the appropriate host manifest for each installed
browser. Native-messaging access is intentionally allow-listed by extension ID;
wildcards are not valid.

## Protocol

All messages are JSON objects with `protocol_version: 1`.

Extension -> host on connection:

```json
{"protocol_version":1,"type":"hello","browser":{"family":"chromium","profile_instance_id":"..."},"capabilities":{"tab_groups":true}}
```

Host -> extension to capture:

```json
{"protocol_version":1,"type":"capture_request","request_id":"..."}
```

Extension -> host on success:

```json
{"protocol_version":1,"type":"capture_result","request_id":"...","browser_session":{}}
```

The native host and desktop-side broker are added in the next slice. This
source package is testable independently with `npm run test:companion`.
