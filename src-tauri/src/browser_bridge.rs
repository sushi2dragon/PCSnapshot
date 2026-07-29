//! Local bridge between the Tauri app and browser-native-messaging hosts.
//!
//! A browser extension is the party that opens native messaging. The desktop
//! app therefore cannot call it directly; each native host attaches to this
//! current-user named pipe and relays bounded JSON requests in both directions.

use crate::{BrowserIdentity, BrowserSession};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::{mpsc, oneshot};

pub const PIPE_NAME: &str = r"\\.\pipe\pc-snapshot-browser-bridge-v1";

#[derive(Clone)]
pub struct BrowserBridge {
    inner: Arc<Inner>,
}

struct Inner {
    next_id: std::sync::atomic::AtomicU64,
    sessions: Mutex<HashMap<String, ConnectedSession>>,
    pending: Mutex<HashMap<String, PendingCapture>>,
    pending_restore: Mutex<HashMap<String, oneshot::Sender<Result<BrowserRestoreReport, String>>>>,
}

struct ConnectedSession {
    connection_id: u64,
    family: String,
    tx: mpsc::UnboundedSender<Value>,
}

struct PendingCapture {
    waiting_for: HashSet<String>,
    sessions: Vec<BrowserSession>,
    errors: Vec<String>,
    complete: oneshot::Sender<CaptureReply>,
}

pub struct CaptureReply {
    pub sessions: Vec<BrowserSession>,
    pub warnings: Vec<String>,
}

#[derive(Deserialize)]
struct BrowserRestoreReport {
    // Reused/opened/skipped are part of the wire protocol and are logged by the
    // extension; only `closed` and `warnings` reach the desktop restore report.
    #[allow(dead_code)]
    reused: u32,
    #[allow(dead_code)]
    opened: u32,
    closed: u32,
    #[allow(dead_code)]
    skipped: u32,
    #[serde(default)]
    warnings: Vec<String>,
}

pub struct RestoreReply {
    pub closed_items: Vec<String>,
    pub warnings: Vec<String>,
}

#[derive(Deserialize)]
struct HelloMessage {
    browser: BrowserIdentity,
}

impl BrowserBridge {
    pub fn start() -> Self {
        let bridge = Self {
            inner: Arc::new(Inner {
                next_id: std::sync::atomic::AtomicU64::new(1),
                sessions: Mutex::new(HashMap::new()),
                pending: Mutex::new(HashMap::new()),
                pending_restore: Mutex::new(HashMap::new()),
            }),
        };

        #[cfg(windows)]
        {
            let server = bridge.clone();
            tauri::async_runtime::spawn(async move { server.serve().await });
        }

        bridge
    }

    /// Ask every currently-connected companion profile for fresh state. A slow
    /// or broken extension becomes a warning; it never blocks desktop capture.
    pub async fn capture(&self, deadline: Duration) -> CaptureReply {
        let request_id = self.next_request_id();

        // A capture fired moments after the desktop app opens races the companion's
        // reconnect: the browser-owned native host reattaches to this pipe within
        // ~1s of the pipe appearing, but the very first capture can arrive before it
        // has. Give a brief grace for at least one profile to register before giving
        // up, so the common "open the app, snapshot right away" path stops spuriously
        // reporting the browser as disconnected. No wait once anything is connected.
        let connect_grace = tokio::time::Instant::now() + Duration::from_millis(1500);
        let targets: Vec<(String, mpsc::UnboundedSender<Value>)> = loop {
            let current: Vec<(String, mpsc::UnboundedSender<Value>)> = self
                .inner
                .sessions
                .lock()
                .expect("browser bridge sessions lock poisoned")
                .iter()
                .map(|(profile, session)| (profile.clone(), session.tx.clone()))
                .collect();
            if !current.is_empty() || tokio::time::Instant::now() >= connect_grace {
                break current;
            }
            tokio::time::sleep(Duration::from_millis(150)).await;
        };

        if targets.is_empty() {
            return CaptureReply {
                sessions: vec![],
                warnings: vec!["Browser Companion is not connected; browser tabs were not captured".to_string()],
            };
        }

        let (complete_tx, complete_rx) = oneshot::channel();
        self.inner
            .pending
            .lock()
            .expect("browser bridge pending lock poisoned")
            .insert(request_id.clone(), PendingCapture {
                waiting_for: targets.iter().map(|(profile, _)| profile.clone()).collect(),
                sessions: vec![],
                errors: vec![],
                complete: complete_tx,
            });

        // Register the pending receiver before sending: an extension can answer
        // fast enough that send-then-register would otherwise drop its result.
        let message = json!({
            "protocol_version": 1,
            "type": "capture_request",
            "request_id": request_id,
        });
        for (profile, tx) in targets {
            if tx.send(message.clone()).is_err() {
                self.fail_capture(
                    &request_id,
                    &profile,
                    "disconnected before capture could start".to_string(),
                );
            }
        }

        match tokio::time::timeout(deadline, complete_rx).await {
            Ok(Ok(reply)) => reply,
            Ok(Err(_)) => CaptureReply {
                sessions: vec![],
                warnings: vec!["Browser Companion capture channel closed unexpectedly".to_string()],
            },
            Err(_) => self.timeout_capture(&request_id),
        }
    }

    /// Ask each captured browser profile to reconcile its own tabs and windows.
    /// A profile that is not connected is reported, never substituted with a
    /// similarly-named browser process or a different profile.
    pub async fn restore(&self, targets: &[BrowserSession], close_extras: bool) -> RestoreReply {
        let mut reply = RestoreReply { closed_items: vec![], warnings: vec![] };
        for target in targets {
            match self.restore_one(target, close_extras, Duration::from_secs(12)).await {
                // A successful reconciliation is not a warning. Reporting its
                // reused/opened counts as one made every browser restore surface
                // the warning overlay, which trained the user to read a working
                // restore as a failure. Only real problems the extension reports
                // are warnings now; the closed count is normal restore output.
                Ok(report) => {
                    if report.closed > 0 {
                        reply.closed_items.push(format!(
                            "{} browser tab(s) closed in {}",
                            report.closed, target.browser.family
                        ));
                    }
                    reply.warnings.extend(report.warnings);
                }
                Err(error) => reply.warnings.push(format!(
                    "Browser session for {} was not restored: {error}",
                    target.browser.family
                )),
            }
        }
        reply
    }

    async fn restore_one(
        &self,
        target: &BrowserSession,
        close_extras: bool,
        deadline: Duration,
    ) -> Result<BrowserRestoreReport, String> {
        let profile = &target.browser.profile_instance_id;
        // Restore may have just cold-launched this browser: a fresh Chromium start
        // plus MV3 service-worker init plus the native-messaging handshake routinely
        // takes well over 5s, which made cold restores falsely report the companion
        // as "not connected" and skip tab reconciliation. Wait long enough to cover a
        // cold start; the full deadline only elapses when the companion is truly absent.
        let connect_deadline = tokio::time::Instant::now() + Duration::from_secs(20);
        let tx = loop {
            if let Some(sender) = self.resolve_target(profile, &target.browser.family) {
                break sender;
            }
            if tokio::time::Instant::now() >= connect_deadline {
                return Err("the companion extension is not connected for this browser profile".to_string());
            }
            tokio::time::sleep(Duration::from_millis(250)).await;
        };
        let request_id = self.next_request_id();
        let (complete_tx, complete_rx) = oneshot::channel();
        self.inner.pending_restore.lock().expect("browser bridge restore lock poisoned")
            .insert(request_id.clone(), complete_tx);
        let message = json!({
            "protocol_version": 1,
            "type": "restore_request",
            "request_id": request_id,
            "browser_session": target,
            "close_extras": close_extras,
        });
        if tx.send(message).is_err() {
            self.inner.pending_restore.lock().expect("browser bridge restore lock poisoned").remove(&request_id);
            return Err("the companion extension disconnected before restore could start".to_string());
        }
        match tokio::time::timeout(deadline, complete_rx).await {
            Ok(Ok(result)) => result,
            Ok(Err(_)) => Err("the companion extension restore channel closed unexpectedly".to_string()),
            Err(_) => {
                self.inner.pending_restore.lock().expect("browser bridge restore lock poisoned").remove(&request_id);
                Err("timed out waiting for the companion extension".to_string())
            }
        }
    }

    /// Browser families with a live companion connection right now, for the
    /// Settings page's one-glance "is this working?" answer.
    pub fn connected_families(&self) -> Vec<String> {
        let sessions = self.inner.sessions.lock().expect("browser bridge sessions lock poisoned");
        let mut families: Vec<String> =
            sessions.values().map(|session| session.family.clone()).collect();
        families.sort();
        families.dedup();
        families
    }

    /// Find the live connection that should service a captured browser profile.
    ///
    /// The profile id lives in the extension's own storage, so reinstalling the
    /// companion — or clearing its data — mints a new one and would otherwise
    /// strand every snapshot captured before that point behind a permanent
    /// "not connected for this browser profile". Falling back to the family match
    /// only when exactly one connected profile shares the family keeps that safe:
    /// tabs can never be reconciled into a different profile than intended.
    fn resolve_target(&self, profile: &str, family: &str) -> Option<mpsc::UnboundedSender<Value>> {
        let sessions = self.inner.sessions.lock().expect("browser bridge sessions lock poisoned");
        if let Some(session) = sessions.get(profile) {
            return Some(session.tx.clone());
        }
        let mut same_family = sessions
            .values()
            .filter(|session| session.family.eq_ignore_ascii_case(family));
        let only = same_family.next()?;
        if same_family.next().is_some() {
            return None;
        }
        Some(only.tx.clone())
    }

    fn next_request_id(&self) -> String {
        let id = self.inner.next_id.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        format!("browser-capture-{id}")
    }

    fn timeout_capture(&self, request_id: &str) -> CaptureReply {
        let Some(pending) = self
            .inner
            .pending
            .lock()
            .expect("browser bridge pending lock poisoned")
            .remove(request_id) else {
                return CaptureReply {
                    sessions: vec![],
                    warnings: vec!["Browser Companion capture timed out".to_string()],
                };
            };

        let mut warnings = pending.errors;
        if !pending.waiting_for.is_empty() {
            warnings.push(format!(
                "Browser Companion did not respond for {} profile(s); their tabs were not captured",
                pending.waiting_for.len()
            ));
        }
        CaptureReply { sessions: pending.sessions, warnings }
    }

    fn finish_capture(&self, request_id: &str, profile_id: &str, session: BrowserSession) {
        let reply = {
            let mut pending = self.inner.pending.lock().expect("browser bridge pending lock poisoned");
            let Some(entry) = pending.get_mut(request_id) else { return; };
            if !entry.waiting_for.remove(profile_id) {
                return;
            }
            entry.sessions.push(session);
            if !entry.waiting_for.is_empty() {
                return;
            }
            pending.remove(request_id)
        };
        if let Some(done) = reply {
            let _ = done.complete.send(CaptureReply { sessions: done.sessions, warnings: done.errors });
        }
    }

    fn fail_capture(&self, request_id: &str, profile_id: &str, detail: String) {
        let reply = {
            let mut pending = self.inner.pending.lock().expect("browser bridge pending lock poisoned");
            let Some(entry) = pending.get_mut(request_id) else { return; };
            if !entry.waiting_for.remove(profile_id) {
                return;
            }
            entry.errors.push(format!("Browser Companion ({profile_id}): {detail}"));
            if !entry.waiting_for.is_empty() {
                return;
            }
            pending.remove(request_id)
        };
        if let Some(done) = reply {
            let _ = done.complete.send(CaptureReply { sessions: done.sessions, warnings: done.errors });
        }
    }

    fn finish_restore(&self, request_id: &str, report: Result<BrowserRestoreReport, String>) {
        if let Some(done) = self.inner.pending_restore.lock().expect("browser bridge restore lock poisoned").remove(request_id) {
            let _ = done.send(report);
        }
    }

    #[cfg(windows)]
    async fn serve(&self) {
        use tokio::net::windows::named_pipe::ServerOptions;

        loop {
            let Ok(server) = ServerOptions::new().create(PIPE_NAME) else {
                // A transient startup race must not take down the desktop app.
                tokio::time::sleep(Duration::from_millis(250)).await;
                continue;
            };
            let Ok(()) = server.connect().await else { continue; };
            let bridge = self.clone();
            tauri::async_runtime::spawn(async move { bridge.handle_client(server).await });
        }
    }

    #[cfg(windows)]
    async fn handle_client(&self, stream: tokio::net::windows::named_pipe::NamedPipeServer) {
        use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

        let connection_id = self.inner.next_id.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let (read, mut write) = tokio::io::split(stream);
        let (tx, mut rx) = mpsc::unbounded_channel::<Value>();
        let writer = tauri::async_runtime::spawn(async move {
            while let Some(message) = rx.recv().await {
                let Ok(bytes) = serde_json::to_vec(&message) else { continue; };
                if write.write_all(&bytes).await.is_err() || write.write_all(b"\n").await.is_err() {
                    break;
                }
            }
        });

        let mut profile_id: Option<String> = None;
        let mut lines = BufReader::new(read).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            let Ok(message) = serde_json::from_str::<Value>(&line) else { continue; };
            if message.get("protocol_version").and_then(Value::as_u64) != Some(1) {
                continue;
            }
            match message.get("type").and_then(Value::as_str) {
                Some("hello") => {
                    let Ok(hello) = serde_json::from_value::<HelloMessage>(message) else { continue; };
                    if hello.browser.profile_instance_id.is_empty() { continue; }
                    let key = hello.browser.profile_instance_id;
                    self.inner.sessions.lock().expect("browser bridge sessions lock poisoned").insert(
                        key.clone(),
                        ConnectedSession { connection_id, family: hello.browser.family, tx: tx.clone() },
                    );
                    profile_id = Some(key);
                }
                Some("capture_result") => {
                    let Some(profile) = profile_id.as_deref() else { continue; };
                    let Some(request_id) = message.get("request_id").and_then(Value::as_str) else { continue; };
                    let Some(raw_session) = message.get("browser_session").cloned() else { continue; };
                    match serde_json::from_value::<BrowserSession>(raw_session) {
                        Ok(session) if session.browser.profile_instance_id == profile => {
                            self.finish_capture(request_id, profile, session)
                        }
                        Ok(_) => self.fail_capture(request_id, profile, "profile identity mismatch".to_string()),
                        Err(_) => self.fail_capture(request_id, profile, "malformed browser session".to_string()),
                    }
                }
                Some("capture_error") => {
                    let Some(profile) = profile_id.as_deref() else { continue; };
                    let Some(request_id) = message.get("request_id").and_then(Value::as_str) else { continue; };
                    let detail = message.get("message").and_then(Value::as_str).unwrap_or("extension capture failed");
                    self.fail_capture(request_id, profile, detail.to_string());
                }
                Some("restore_result") => {
                    let Some(request_id) = message.get("request_id").and_then(Value::as_str) else { continue; };
                    let Some(raw_report) = message.get("report").cloned() else { continue; };
                    let report = serde_json::from_value(raw_report)
                        .map_err(|_| "malformed browser restore report".to_string());
                    self.finish_restore(request_id, report);
                }
                Some("restore_error") => {
                    let Some(request_id) = message.get("request_id").and_then(Value::as_str) else { continue; };
                    let detail = message.get("message").and_then(Value::as_str)
                        .unwrap_or("extension restore failed").to_string();
                    self.finish_restore(request_id, Err(detail));
                }
                _ => {}
            }
        }

        if let Some(profile) = profile_id {
            let mut sessions = self.inner.sessions.lock().expect("browser bridge sessions lock poisoned");
            if sessions.get(&profile).is_some_and(|session| session.connection_id == connection_id) {
                sessions.remove(&profile);
            }
        }
        writer.abort();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bridge_for_test() -> BrowserBridge {
        BrowserBridge {
            inner: Arc::new(Inner {
                next_id: std::sync::atomic::AtomicU64::new(1),
                sessions: Mutex::new(HashMap::new()),
                pending: Mutex::new(HashMap::new()),
                pending_restore: Mutex::new(HashMap::new()),
            }),
        }
    }

    fn session(profile: &str) -> BrowserSession {
        serde_json::from_value(json!({
            "protocol_version": 1,
            "browser": { "family": "chrome", "profile_instance_id": profile },
            "captured_at": "2026-01-01T00:00:00Z",
            "capabilities": { "tab_groups": true },
            "windows": []
        })).expect("valid test browser session")
    }

    fn connect(bridge: &BrowserBridge, profile: &str, family: &str) {
        let (tx, rx) = mpsc::unbounded_channel();
        // Hold the receiver open for the life of the test session.
        std::mem::forget(rx);
        bridge.inner.sessions.lock().unwrap().insert(
            profile.to_string(),
            ConnectedSession { connection_id: 1, family: family.to_string(), tx },
        );
    }

    #[test]
    fn restore_falls_back_to_the_only_profile_of_that_browser_family() {
        // A reinstalled companion mints a new profile id, so a snapshot captured
        // before the reinstall can no longer match by id. One connected Chrome is
        // unambiguous, so it must still be restorable.
        let bridge = bridge_for_test();
        connect(&bridge, "profile-new", "chrome");
        assert!(bridge.resolve_target("profile-old", "chrome").is_some());
    }

    #[test]
    fn restore_never_guesses_between_two_profiles_of_the_same_family() {
        let bridge = bridge_for_test();
        connect(&bridge, "profile-a", "chrome");
        connect(&bridge, "profile-b", "chrome");
        assert!(bridge.resolve_target("profile-old", "chrome").is_none());
    }

    #[test]
    fn restore_never_crosses_browser_families() {
        let bridge = bridge_for_test();
        connect(&bridge, "profile-edge", "edge");
        assert!(bridge.resolve_target("profile-old", "chrome").is_none());
    }

    #[test]
    fn an_exact_profile_match_always_wins_over_the_family_fallback() {
        let bridge = bridge_for_test();
        connect(&bridge, "profile-exact", "chrome");
        connect(&bridge, "profile-other", "chrome");
        assert!(bridge.resolve_target("profile-exact", "chrome").is_some());
    }

    #[test]
    fn capture_completes_only_after_every_connected_profile_replies() {
        let bridge = bridge_for_test();
        let (tx, mut rx) = oneshot::channel();
        bridge.inner.pending.lock().unwrap().insert("capture-1".to_string(), PendingCapture {
            waiting_for: ["profile-a".to_string(), "profile-b".to_string()].into_iter().collect(),
            sessions: vec![],
            errors: vec![],
            complete: tx,
        });

        bridge.finish_capture("capture-1", "profile-a", session("profile-a"));
        assert!(rx.try_recv().is_err(), "one profile cannot complete a multi-profile capture");

        bridge.finish_capture("capture-1", "profile-b", session("profile-b"));
        let reply = rx.try_recv().expect("all profiles must complete the capture");
        assert_eq!(reply.sessions.len(), 2);
    }
}

#[cfg(not(windows))]
impl BrowserBridge {
    pub async fn unavailable_capture(&self) -> CaptureReply {
        CaptureReply { sessions: vec![], warnings: vec!["Browser Companion is only supported on Windows".to_string()] }
    }
}
