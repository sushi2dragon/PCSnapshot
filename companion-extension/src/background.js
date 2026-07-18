import { captureBrowserSession } from "./capture.js";
import { reconcileBrowserSession } from "./restore.js";

const HOST_NAME = "app.pcsnapshot.companion";
const api = globalThis.browser ?? globalThis.chrome;
let port = null;

async function profileInstanceId() {
  const stored = await api.storage.local.get("profile_instance_id");
  if (typeof stored.profile_instance_id === "string" && stored.profile_instance_id) {
    return stored.profile_instance_id;
  }
  const id = globalThis.crypto.randomUUID();
  await api.storage.local.set({ profile_instance_id: id });
  return id;
}

async function browserFamily() {
  if (typeof api.runtime.getBrowserInfo === "function") {
    const info = await api.runtime.getBrowserInfo();
    return String(info.name ?? "firefox").toLowerCase();
  }
  const ua = globalThis.navigator.userAgent;
  if (/edg\//i.test(ua)) return "edge";
  if (/opr\//i.test(ua)) return "opera";
  if (/brave/i.test(ua)) return "brave";
  if (/vivaldi/i.test(ua)) return "vivaldi";
  return "chromium";
}

function isCaptureRequest(message) {
  return message?.protocol_version === 1
    && message?.type === "capture_request"
    && typeof message?.request_id === "string";
}

function isRestoreRequest(message) {
  return message?.protocol_version === 1
    && message?.type === "restore_request"
    && typeof message?.request_id === "string";
}

async function replyToCapture(message) {
  console.debug("PC Snapshot capture requested", { requestId: message.request_id });
  const [family, profileId] = await Promise.all([browserFamily(), profileInstanceId()]);
  const browserSession = await captureBrowserSession(api, family, profileId);
  port?.postMessage({
    protocol_version: 1,
    type: "capture_result",
    request_id: message.request_id,
    browser_session: browserSession,
  });
  console.debug("PC Snapshot capture completed", {
    requestId: message.request_id,
    windowCount: browserSession.windows.length,
  });
}

async function replyToRestore(message) {
  console.debug("PC Snapshot browser restore started", {
    requestId: message.request_id,
    closeExtras: message.close_extras,
  });
  const report = await reconcileBrowserSession(
    api,
    message.browser_session,
    message.close_extras,
  );
  port?.postMessage({
    protocol_version: 1,
    type: "restore_result",
    request_id: message.request_id,
    report,
  });
  console.debug("PC Snapshot browser restore completed", {
    requestId: message.request_id,
    report,
  });
}

async function connectNative() {
  if (port) return;
  try {
    const nextPort = api.runtime.connectNative(HOST_NAME);
    port = nextPort;
    nextPort.onDisconnect.addListener(() => {
      if (port === nextPort) port = null;
    });
    nextPort.onMessage.addListener((message) => {
      if (isCaptureRequest(message)) {
        replyToCapture(message).catch((error) => {
          nextPort.postMessage({
            protocol_version: 1,
            type: "capture_error",
            request_id: message.request_id,
            message: error instanceof Error ? error.message : "Browser operation failed",
          });
        });
        return;
      }
      if (!isRestoreRequest(message)) return;
      replyToRestore(message).catch((error) => {
        const errorMessage = error instanceof Error ? error.message : "Browser restore failed";
        console.error("PC Snapshot browser restore failed", {
          requestId: message.request_id,
          error,
        });
        nextPort.postMessage({
          protocol_version: 1,
          type: "restore_error",
          request_id: message.request_id,
          message: errorMessage,
        });
      });
    });

    const [family, profileId] = await Promise.all([browserFamily(), profileInstanceId()]);
    nextPort.postMessage({
      protocol_version: 1,
      type: "hello",
      browser: {
        family,
        extension_id: api.runtime.id,
        profile_instance_id: profileId,
      },
      capabilities: { tab_groups: typeof api.tabGroups?.query === "function" },
    });
    console.debug("PC Snapshot native host connected", { family, profileId });
  } catch (error) {
    // The host is registered by the desktop installer; retry on the next
    // extension/browser lifecycle event rather than polling in the background.
    console.error("PC Snapshot native host connection failed", {
      error: error instanceof Error ? error.message : error,
      lastError: api.runtime.lastError?.message,
    });
    port = null;
  }
}

// The MV3 service worker is torn down after ~30s idle, which drops the native
// connection; nothing re-establishes it until the extension is manually reloaded.
// A short-period alarm both wakes the worker (each firing resets the idle timer)
// and reconnects if the port was lost — no user interaction, install-and-forget.
// The desktop host also heartbeats to keep the worker warm between alarm ticks.
const KEEPALIVE_ALARM = "pcs-companion-keepalive";

function ensureKeepaliveAlarm() {
  api.alarms.create(KEEPALIVE_ALARM, { periodInMinutes: 0.4 });
}

api.alarms.onAlarm.addListener((alarm) => {
  if (alarm.name === KEEPALIVE_ALARM) void connectNative();
});

// The alarm is a 30s floor (Chrome clamps sub-30s periods), and the host heartbeat
// only keeps the worker warm while the desktop app is running to send it. So when
// the app is closed, the worker dies every ~30s; reopening the app and capturing
// right away finds a dead port and no connected session — the "reload the extension
// to fix it" symptom. Reconnecting on ordinary browser activity closes that gap:
// any tab switch, navigation, or window focus while the user is in the browser wakes
// the worker and re-establishes the native port immediately instead of waiting for
// the next alarm tick. connectNative() is a no-op when already connected, so these
// high-frequency events are cheap.
function wakeAndConnect() { void connectNative(); }
api.tabs.onActivated.addListener(wakeAndConnect);
api.tabs.onUpdated.addListener(wakeAndConnect);
if (api.windows?.onFocusChanged) api.windows.onFocusChanged.addListener(wakeAndConnect);

api.runtime.onInstalled.addListener(() => { ensureKeepaliveAlarm(); void connectNative(); });
api.runtime.onStartup.addListener(() => { ensureKeepaliveAlarm(); void connectNative(); });
ensureKeepaliveAlarm();
void connectNative();
