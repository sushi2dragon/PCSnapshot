//! Browser Companion setup: native-messaging host registration.
//!
//! The companion needs two things on a machine: the extension (installed by the
//! user in their browser) and a native-messaging host manifest that tells the
//! browser which executable to launch and which extension may talk to it. The
//! second half used to be a hand-run PowerShell script, which meant the manifest
//! pointed at whatever build path happened to be current when it was run — a
//! stale path silently breaks every capture and restore.
//!
//! The app now writes that registration itself on every launch, from its own
//! resolved executable location, so the manifest can never drift from the
//! installed binary and the user never runs a script.

const HOST_NAME: &str = "app.pcsnapshot.companion";
const HOST_EXE: &str = "pc_snapshot_native_host.exe";
const CHROMIUM_EXTENSION_ID: &str = "chfbdgfhlkbocpeofdjkincopepifnlj";
const FIREFOX_EXTENSION_ID: &str = "pc-snapshot-companion@pcsnapshot.app";

/// What the UI needs to tell the user whether the companion can work at all.
#[derive(serde::Serialize, Clone)]
pub struct CompanionStatus {
    /// The relay executable the browser launches. Absent means a broken build or
    /// install; nothing else about the companion can work without it.
    pub host_installed: bool,
    pub host_path: String,
    /// Browsers whose native-messaging registration this machine now carries.
    pub registered_browsers: Vec<String>,
    pub errors: Vec<String>,
}

/// Everything the Settings page needs to show one honest companion state and one
/// action: registration health from app launch plus who is connected right now.
#[derive(serde::Serialize, Clone)]
pub struct CompanionReport {
    pub host_installed: bool,
    pub host_path: String,
    pub registered_browsers: Vec<String>,
    pub errors: Vec<String>,
    /// Browser families with a live companion connection this instant.
    pub connected_browsers: Vec<String>,
}

impl CompanionStatus {
    pub fn into_report(self, connected_browsers: Vec<String>) -> CompanionReport {
        CompanionReport {
            host_installed: self.host_installed,
            host_path: self.host_path,
            registered_browsers: self.registered_browsers,
            errors: self.errors,
            connected_browsers,
        }
    }
}

/// Path of the native-messaging relay that ships beside the app executable.
pub fn host_path() -> std::path::PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|dir| dir.join(HOST_EXE)))
        .unwrap_or_else(|| std::path::PathBuf::from(HOST_EXE))
}

/// Directory holding the generated host manifests. Per-user by design: the
/// companion never needs elevation, so setup can be silent at every launch.
#[cfg(windows)]
fn manifest_dir() -> Option<std::path::PathBuf> {
    std::env::var("LOCALAPPDATA")
        .ok()
        .map(|base| std::path::PathBuf::from(base).join("PC Snapshot").join("BrowserCompanion"))
}

/// Chromium-family registry roots that hold per-user native-messaging hosts.
/// Written whether or not the browser is currently installed: the registration
/// is inert without the browser, and installing one later then works with no
/// second setup step.
#[cfg(windows)]
const CHROMIUM_ROOTS: &[(&str, &str)] = &[
    ("Chrome", r"Software\Google\Chrome\NativeMessagingHosts"),
    ("Edge", r"Software\Microsoft\Edge\NativeMessagingHosts"),
    ("Brave", r"Software\BraveSoftware\Brave-Browser\NativeMessagingHosts"),
    ("Vivaldi", r"Software\Vivaldi\NativeMessagingHosts"),
    ("Opera", r"Software\Opera Software\Opera Stable\NativeMessagingHosts"),
    ("Opera GX", r"Software\Opera Software\Opera GX Stable\NativeMessagingHosts"),
];

#[cfg(windows)]
const FIREFOX_ROOT: (&str, &str) = ("Firefox", r"Software\Mozilla\NativeMessagingHosts");

/// Write the host manifests and point every supported browser at them.
///
/// Idempotent and best-effort: a browser whose registry root cannot be written
/// becomes an entry in `errors`, never a failed app launch.
#[cfg(windows)]
pub fn register() -> CompanionStatus {
    use winreg::enums::{HKEY_CURRENT_USER, KEY_WRITE};
    use winreg::RegKey;

    let host = host_path();
    let mut status = CompanionStatus {
        host_installed: host.is_file(),
        host_path: host.to_string_lossy().into_owned(),
        registered_browsers: vec![],
        errors: vec![],
    };

    let Some(dir) = manifest_dir() else {
        status.errors.push("Could not locate the local application data folder".to_string());
        return status;
    };
    if let Err(error) = std::fs::create_dir_all(&dir) {
        status.errors.push(format!("Could not create the companion folder: {error}"));
        return status;
    }

    let chromium_manifest = dir.join("chromium-host.json");
    let firefox_manifest = dir.join("firefox-host.json");
    let host_string = host.to_string_lossy().into_owned();

    let chromium_body = serde_json::json!({
        "name": HOST_NAME,
        "description": "PC Snapshot Browser Companion",
        "path": host_string,
        "type": "stdio",
        "allowed_origins": [format!("chrome-extension://{CHROMIUM_EXTENSION_ID}/")],
    });
    let firefox_body = serde_json::json!({
        "name": HOST_NAME,
        "description": "PC Snapshot Browser Companion",
        "path": host_string,
        "type": "stdio",
        "allowed_extensions": [FIREFOX_EXTENSION_ID],
    });

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    for (manifest_path, body, targets) in [
        (&chromium_manifest, &chromium_body, CHROMIUM_ROOTS),
        (&firefox_manifest, &firefox_body, std::slice::from_ref(&FIREFOX_ROOT)),
    ] {
        let json = match serde_json::to_string_pretty(body) {
            Ok(json) => json,
            Err(error) => {
                status.errors.push(format!("Could not build a host manifest: {error}"));
                continue;
            }
        };
        if let Err(error) = std::fs::write(manifest_path, json) {
            status.errors.push(format!(
                "Could not write {}: {error}",
                manifest_path.display()
            ));
            continue;
        }
        let manifest_string = manifest_path.to_string_lossy().into_owned();
        for (label, root) in targets {
            let key_path = format!(r"{root}\{HOST_NAME}");
            match hkcu.create_subkey_with_flags(&key_path, KEY_WRITE) {
                Ok((key, _)) => match key.set_value("", &manifest_string) {
                    Ok(()) => status.registered_browsers.push((*label).to_string()),
                    Err(error) => status
                        .errors
                        .push(format!("Could not register the companion for {label}: {error}")),
                },
                Err(error) => status
                    .errors
                    .push(format!("Could not register the companion for {label}: {error}")),
            }
        }
    }

    if !status.host_installed {
        status.errors.push(format!(
            "The companion relay is missing at {}; browser capture and restore will not run",
            status.host_path
        ));
    }
    status
}

// Deliberately untested by `cargo test`: registration resolves its host path from
// `current_exe()` and writes the real HKCU hive, so a test would both read the
// wrong executable (the test harness) and clobber the machine's live setup. It is
// verified by launching the app and reading Settings → Terminal & Browser.

#[cfg(not(windows))]
pub fn register() -> CompanionStatus {
    CompanionStatus {
        host_installed: false,
        host_path: host_path().to_string_lossy().into_owned(),
        registered_browsers: vec![],
        errors: vec!["The Browser Companion is only supported on Windows".to_string()],
    }
}
