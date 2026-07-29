# PC Snapshot Companion

The PC Snapshot Companion is a WebExtension that reports structured browser
state to the desktop app through native messaging. It is deliberately local:
it has no network access, content scripts, cookie access, or history access.

## What it does

- Captures normal (non-private) browser windows and their bounds.
- Captures every tab's URL, title, order, active/pinned/muted/discarded state,
  and a snapshot-local tab-group key.
- Captures group title, color, collapsed state, and order when the browser
  supports the tab-groups API.
- Reconciles a captured session on restore: reuse tabs whose URL already
  matches, open the missing ones, move everything into snapshot order, and close
  the extras when "Close others" is on. Tabs are always created before extras are
  removed, so a failure can never empty the browser.
- Keeps one persistent native-messaging port open and reconnects itself — no
  reload, no user interaction.

## Staying connected

Manifest V3 shuts a service worker down after ~30s idle, which drops the native
port. Three things keep the companion live, in order of importance:

1. The native host sends a heartbeat every 20s **whether or not PC Snapshot is
   running**. Each message resets the worker's idle timer.
2. The host reconnects to the desktop bridge instead of exiting when the app
   closes, and replays the extension's `hello` on the new connection, so closing
   and reopening PC Snapshot never leaves the profile unregistered.
3. The worker reconnects on port disconnect (with backoff), on a keepalive
   alarm, and on ordinary browser activity.

## Installation

The desktop app registers the native-messaging host itself on every launch,
writing the manifest from its own resolved executable path into
`%LOCALAPPDATA%\PC Snapshot\BrowserCompanion\` and pointing every supported
browser's `HKCU` `NativeMessagingHosts` key at it. Nothing to run by hand, and
the registration can never go stale against a moved or reinstalled binary.
Settings → Terminal & Browser shows the live state.

Installing the extension is the one remaining user step.

## Builds

`npm run build:companion` writes an unpacked Chromium package to
`companion-extension/dist/chromium`. `manifest.chromium.json` carries a fixed
`key`, so the extension ID stays `chfbdgfhlkbocpeofdjkincopepifnlj` across
reloads and the host allow-list keeps matching. `manifest.firefox.json` is for
Firefox and has its own fixed add-on ID. Native-messaging access is
allow-listed by extension ID; wildcards are not valid.

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

Extension -> host on restore:

```json
{"protocol_version":1,"type":"restore_result","request_id":"...","report":{"reused":0,"opened":0,"closed":0,"skipped":0,"ignored":0,"warnings":[]}}
```

`skipped` means "should have been restored and wasn't" and always has a matching
warning. `ignored` counts pages that were never restorable (new-tab, settings,
devtools) and is deliberately not a warning — every profile has some.

Host -> extension every 20s:

```json
{"protocol_version":1,"type":"heartbeat"}
```

This source package is testable independently with `npm run test:companion`.
