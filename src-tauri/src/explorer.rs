//! File Explorer folder-window capture and restore.
//!
//! Explorer is a special case: folder windows share `explorer.exe` with the
//! taskbar, Start menu, and desktop. The process therefore remains protected
//! everywhere else; this module talks to the Shell COM automation surface and
//! manages only concrete folder-window HWNDs.

use crate::ExplorerWindow;

#[derive(Clone, Debug)]
pub(crate) struct LiveExplorerWindow {
    pub hwnd: isize,
    pub path: String,
    pub path_kind: String,
    /// Whether this location has a stable target we can persist and reopen.
    /// Unsupported locations still remain closeable during Start New.
    pub restorable: bool,
}

pub(crate) struct ExplorerQuery {
    pub windows: Vec<LiveExplorerWindow>,
    pub warnings: Vec<String>,
}

pub(crate) struct ExplorerRestoreOutcome {
    pub failed_items: Vec<String>,
    pub warnings: Vec<String>,
    pub closed_items: Vec<String>,
}

/// Query ShellWindows on a dedicated STA. Capture can run on a Tauri runtime
/// thread whose COM apartment is not under our control.
#[cfg(windows)]
pub(crate) fn query_live_windows() -> Result<ExplorerQuery, String> {
    std::thread::Builder::new()
        .name("pc-snapshot-explorer-query".to_string())
        .spawn(query_live_windows_sta)
        .map_err(|e| format!("could not start Explorer query: {e}"))?
        .join()
        .map_err(|_| "Explorer query thread panicked".to_string())?
}

#[cfg(windows)]
fn query_live_windows_sta() -> Result<ExplorerQuery, String> {
    use windows::core::{Interface, VARIANT};
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_LOCAL_SERVER,
        COINIT_APARTMENTTHREADED,
    };
    use windows::Win32::UI::Shell::{IShellWindows, IWebBrowser2, ShellWindows};

    unsafe {
        let hr = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        hr.ok()
            .map_err(|e| format!("could not initialize Shell COM: {e}"))?;

        let result = (|| -> Result<ExplorerQuery, String> {
            let shell: IShellWindows =
                CoCreateInstance(&ShellWindows, None, CLSCTX_LOCAL_SERVER)
                    .map_err(|e| format!("could not access open Explorer windows: {e}"))?;
            let count = shell
                .Count()
                .map_err(|e| format!("could not count open Explorer windows: {e}"))?;
            let mut windows = Vec::new();
            let mut warnings = Vec::new();

            for index in 0..count {
                let Ok(dispatch) = shell.Item(&VARIANT::from(index)) else {
                    continue;
                };
                let Ok(browser) = dispatch.cast::<IWebBrowser2>() else {
                    continue;
                };

                // ShellWindows can contain non-Explorer automation servers.
                let full_name = browser
                    .FullName()
                    .map(|value| value.to_string())
                    .unwrap_or_default();
                if !is_explorer_executable(&full_name) {
                    continue;
                }

                let Ok(hwnd) = browser.HWND() else {
                    continue;
                };
                let location_name = browser
                    .LocationName()
                    .map(|value| value.to_string())
                    .unwrap_or_default();
                let location_url = browser
                    .LocationURL()
                    .map(|value| value.to_string())
                    .unwrap_or_default();

                match restorable_target(&location_url, &location_name) {
                    Some((path, path_kind)) => windows.push(LiveExplorerWindow {
                        hwnd: hwnd.0,
                        path,
                        path_kind: path_kind.to_string(),
                        restorable: true,
                    }),
                    None => {
                        let label = if location_name.is_empty() {
                            "(unknown)".to_string()
                        } else {
                            location_name
                        };
                        warnings.push(format!(
                            "File Explorer location '{label}' is not restorable and was skipped"
                        ));
                        windows.push(LiveExplorerWindow {
                            hwnd: hwnd.0,
                            path: label,
                            path_kind: "unsupported".to_string(),
                            restorable: false,
                        });
                    }
                }
            }

            Ok(ExplorerQuery { windows, warnings })
        })();

        CoUninitialize();
        result
    }
}

#[cfg(not(windows))]
pub(crate) fn query_live_windows() -> Result<ExplorerQuery, String> {
    Ok(ExplorerQuery {
        windows: vec![],
        warnings: vec![],
    })
}

#[cfg(windows)]
pub(crate) fn restore_windows(
    saved: &[ExplorerWindow],
    close_extras: bool,
) -> ExplorerRestoreOutcome {
    use std::collections::HashSet;
    use std::time::{Duration, Instant};

    let mut outcome = ExplorerRestoreOutcome {
        failed_items: vec![],
        warnings: vec![],
        closed_items: vec![],
    };

    if saved.is_empty() && !close_extras {
        return outcome;
    }

    let initial = match query_live_windows() {
        Ok(query) => query,
        Err(error) => {
            outcome.warnings.push(format!(
                "File Explorer windows could not be inspected: {error}"
            ));
            return outcome;
        }
    };

    let mut consumed = HashSet::new();
    let mut pending: Vec<&ExplorerWindow> = Vec::new();
    for target in saved {
        if let Some(live) = find_target(&initial.windows, target, &consumed) {
            apply_geometry(live.hwnd, target);
            consumed.insert(live.hwnd);
        } else {
            if let Err(error) = launch_folder_window(&target.path) {
                outcome.failed_items.push(format!(
                    "File Explorer '{}': failed to open ({error})",
                    target.path
                ));
            } else {
                pending.push(target);
            }
        }
    }

    // Wait for newly requested folder windows to register with ShellWindows,
    // then match one-to-one by path before applying their saved geometry.
    let deadline = Instant::now() + Duration::from_millis(6000);
    while !pending.is_empty() && Instant::now() < deadline {
        if let Ok(query) = query_live_windows() {
            pending.retain(|target| {
                if let Some(live) = find_target(&query.windows, target, &consumed) {
                    apply_geometry(live.hwnd, target);
                    consumed.insert(live.hwnd);
                    false
                } else {
                    true
                }
            });
        }
        if !pending.is_empty() {
            std::thread::sleep(Duration::from_millis(150));
        }
    }

    outcome.warnings.extend(pending.into_iter().map(|target| {
        format!(
            "File Explorer '{}' opened but its window could not be matched for repositioning",
            target.path
        )
    }));

    if close_extras {
        close_extra_windows(saved, &mut outcome);
    }

    outcome
}

/// Close every user-facing Explorer window for Start New. This targets only
/// individual ShellWindows HWNDs; the shared explorer.exe shell process stays
/// protected and is never terminated.
#[cfg(windows)]
pub(crate) fn close_all_folder_windows() -> (Vec<String>, Vec<String>) {
    let mut outcome = ExplorerRestoreOutcome {
        failed_items: vec![],
        warnings: vec![],
        closed_items: vec![],
    };
    close_extra_windows(&[], &mut outcome);
    (outcome.closed_items, outcome.warnings)
}

#[cfg(not(windows))]
pub(crate) fn restore_windows(
    _saved: &[ExplorerWindow],
    _close_extras: bool,
) -> ExplorerRestoreOutcome {
    ExplorerRestoreOutcome {
        failed_items: vec![],
        warnings: vec![],
        closed_items: vec![],
    }
}

#[cfg(not(windows))]
pub(crate) fn close_all_folder_windows() -> (Vec<String>, Vec<String>) {
    (vec![], vec![])
}

#[cfg(windows)]
fn find_target<'a>(
    live: &'a [LiveExplorerWindow],
    saved: &ExplorerWindow,
    consumed: &std::collections::HashSet<isize>,
) -> Option<&'a LiveExplorerWindow> {
    live.iter().find(|candidate| {
        !consumed.contains(&candidate.hwnd)
            && same_target(
                &candidate.path,
                &candidate.path_kind,
                &saved.path,
                &saved.path_kind,
            )
    })
}

#[cfg(windows)]
fn launch_folder_window(target: &str) -> Result<(), String> {
    if target.trim().is_empty() {
        return Err("empty folder target".to_string());
    }
    std::process::Command::new("explorer.exe")
        // /n requests a distinct folder window instead of reusing an existing one.
        .arg("/n,")
        .arg(target)
        .spawn()
        .map(|_| ())
        .map_err(|e| e.to_string())
}

#[cfg(windows)]
fn apply_geometry(hwnd_raw: isize, target: &ExplorerWindow) {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::{
        SetWindowPos, ShowWindow, HWND_TOP, SWP_NOACTIVATE, SWP_NOZORDER, SW_MAXIMIZE, SW_MINIMIZE,
        SW_RESTORE,
    };

    let hwnd = HWND(hwnd_raw as *mut core::ffi::c_void);
    unsafe {
        let _ = ShowWindow(hwnd, SW_RESTORE);
        let _ = SetWindowPos(
            hwnd,
            HWND_TOP,
            target.position.x,
            target.position.y,
            target.size.width as i32,
            target.size.height as i32,
            SWP_NOZORDER | SWP_NOACTIVATE,
        );
        match target.state.as_str() {
            "maximized" => {
                let _ = ShowWindow(hwnd, SW_MAXIMIZE);
            }
            "minimized" => {
                let _ = ShowWindow(hwnd, SW_MINIMIZE);
            }
            _ => {}
        }
    }
}

#[cfg(windows)]
fn close_extra_windows(saved: &[ExplorerWindow], outcome: &mut ExplorerRestoreOutcome) {
    use std::collections::HashSet;
    use std::time::{Duration, Instant};
    use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
    use windows::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_CLOSE};

    let Ok(query) = query_live_windows() else {
        outcome.warnings.push(
            "Extra File Explorer windows could not be inspected for clean restore".to_string(),
        );
        return;
    };
    let mut claimed = HashSet::new();
    for target in saved {
        if let Some(live) = find_target(&query.windows, target, &claimed) {
            claimed.insert(live.hwnd);
        }
    }
    let mut requested = Vec::new();
    for live in unclaimed_live_windows(&query.windows, &claimed) {
        let hwnd = HWND(live.hwnd as *mut core::ffi::c_void);
        if unsafe { PostMessageW(hwnd, WM_CLOSE, WPARAM(0), LPARAM(0)) }.is_ok() {
            requested.push(live.clone());
        } else {
            outcome.warnings.push(format!(
                "Extra File Explorer window '{}' could not be closed",
                live.path
            ));
        }
    }

    // A posted WM_CLOSE is only a request. Report success only after the HWND
    // actually disappears from the authoritative ShellWindows list.
    let mut remaining: HashSet<isize> = requested.iter().map(|window| window.hwnd).collect();
    let deadline = Instant::now() + Duration::from_millis(1200);
    let mut query_failed = false;
    while !remaining.is_empty() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(100));
        match query_live_windows() {
            Ok(current) => {
                let live_handles: HashSet<isize> =
                    current.windows.iter().map(|window| window.hwnd).collect();
                remaining.retain(|hwnd| live_handles.contains(hwnd));
            }
            Err(_) => {
                query_failed = true;
                break;
            }
        }
    }
    for window in requested {
        if !query_failed && !remaining.contains(&window.hwnd) {
            outcome
                .closed_items
                .push(format!("'{}' (File Explorer)", window.path));
        } else {
            outcome.warnings.push(format!(
                "Extra File Explorer window '{}' did not confirm that it closed",
                window.path
            ));
        }
    }
}

fn unclaimed_live_windows<'a>(
    live: &'a [LiveExplorerWindow],
    claimed: &std::collections::HashSet<isize>,
) -> Vec<&'a LiveExplorerWindow> {
    live.iter()
        .filter(|window| !claimed.contains(&window.hwnd))
        .collect()
}

fn is_explorer_executable(path: &str) -> bool {
    path.rsplit(['\\', '/'])
        .next()
        .is_some_and(|name| name.eq_ignore_ascii_case("explorer.exe"))
}

fn restorable_target(url: &str, location_name: &str) -> Option<(String, &'static str)> {
    if let Some(path) = file_url_to_path(url) {
        return Some((path, "filesystem"));
    }

    let url_lower = url.trim().to_ascii_lowercase();
    let name_lower = location_name.trim().to_ascii_lowercase();
    if url_lower.contains("20d04fe0-3aea-1069-a2d8-08002b30309d") || name_lower == "this pc" {
        return Some(("shell:MyComputerFolder".to_string(), "virtual"));
    }
    if url_lower.contains("679f85cb-0220-4080-b29b-5540cc05aab6")
        || name_lower == "quick access"
        || name_lower == "home"
    {
        return Some(("shell:Home".to_string(), "virtual"));
    }
    if url_lower.contains("645ff040-5081-101b-9f08-00aa002f954e") || name_lower == "recycle bin" {
        return Some(("shell:RecycleBinFolder".to_string(), "virtual"));
    }
    None
}

fn file_url_to_path(url: &str) -> Option<String> {
    let prefix = url.get(..7)?;
    if !prefix.eq_ignore_ascii_case("file://") {
        return None;
    }
    let encoded = &url[7..];
    let decoded = percent_decode(encoded)?;
    if decoded.starts_with('/') {
        let mut path = decoded.replace('/', "\\");
        if path.as_bytes().get(2) == Some(&b':') {
            path.remove(0);
        }
        (!path.is_empty()).then_some(path)
    } else {
        let unc = decoded.replace('/', "\\");
        (!unc.is_empty()).then(|| format!("\\\\{unc}"))
    }
}

fn percent_decode(value: &str) -> Option<String> {
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            let hi = *bytes.get(index + 1)?;
            let lo = *bytes.get(index + 2)?;
            decoded.push((hex_value(hi)? << 4) | hex_value(lo)?);
            index += 3;
        } else {
            decoded.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(decoded).ok()
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn same_target(left: &str, left_kind: &str, right: &str, right_kind: &str) -> bool {
    left_kind.eq_ignore_ascii_case(right_kind)
        && left
            .trim_end_matches(['\\', '/'])
            .eq_ignore_ascii_case(right.trim_end_matches(['\\', '/']))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_local_and_unc_file_urls() {
        assert_eq!(
            file_url_to_path("file:///C:/Users/Sarth/My%20Files"),
            Some(r"C:\Users\Sarth\My Files".to_string())
        );
        assert_eq!(
            file_url_to_path("file://server/share/My%20Files"),
            Some(r"\\server\share\My Files".to_string())
        );
    }

    #[test]
    fn rejects_malformed_or_non_file_urls() {
        assert_eq!(file_url_to_path("https://example.com"), None);
        assert_eq!(file_url_to_path("file:///C:/bad%2"), None);
        assert_eq!(file_url_to_path("file:///C:/bad%XZ"), None);
    }

    #[test]
    fn resolves_common_virtual_roots_only() {
        assert_eq!(
            restorable_target("", "This PC"),
            Some(("shell:MyComputerFolder".to_string(), "virtual"))
        );
        assert_eq!(
            restorable_target("", "Home"),
            Some(("shell:Home".to_string(), "virtual"))
        );
        assert_eq!(restorable_target("", "Search Results in Documents"), None);
    }

    #[test]
    fn target_matching_is_case_and_trailing_separator_insensitive() {
        assert!(same_target(
            r"C:\Users\Sarth\Downloads\",
            "filesystem",
            r"c:\users\sarth\downloads",
            "FILESYSTEM"
        ));
        assert!(!same_target(
            r"C:\Users\Sarth\Downloads",
            "filesystem",
            r"C:\Users\Sarth\Documents",
            "filesystem"
        ));
    }

    #[test]
    fn start_new_targets_every_enumerated_explorer_window() {
        let live = vec![
            LiveExplorerWindow {
                hwnd: 10,
                path: r"C:\Downloads".to_string(),
                path_kind: "filesystem".to_string(),
                restorable: true,
            },
            LiveExplorerWindow {
                hwnd: 20,
                path: "Search Results".to_string(),
                path_kind: "unsupported".to_string(),
                restorable: false,
            },
            LiveExplorerWindow {
                hwnd: 30,
                path: r"C:\Scripts".to_string(),
                path_kind: "filesystem".to_string(),
                restorable: true,
            },
        ];
        let selected: Vec<isize> = unclaimed_live_windows(&live, &Default::default())
            .into_iter()
            .map(|window| window.hwnd)
            .collect();
        assert_eq!(selected, vec![10, 20, 30]);
    }

    #[cfg(windows)]
    #[test]
    #[ignore = "requires an interactive Windows desktop with File Explorer open"]
    fn production_query_sees_an_open_folder_window() {
        let query = query_live_windows().expect("Shell COM query should succeed");
        assert!(
            !query.windows.is_empty(),
            "open a File Explorer folder before running this ignored smoke test"
        );
    }

    #[cfg(windows)]
    #[test]
    #[ignore = "opens and closes a temporary File Explorer window"]
    fn production_restore_opens_matches_and_closes_a_folder_window() {
        use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
        use windows::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_CLOSE};

        let folder = std::env::temp_dir().join(format!(
            "pc_snapshot_explorer_restore_smoke_{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&folder).unwrap();
        let path = folder.to_string_lossy().into_owned();
        let target = ExplorerWindow {
            path: path.clone(),
            path_kind: "filesystem".to_string(),
            title: "Explorer restore smoke".to_string(),
            position: crate::WindowPosition { x: 80, y: 80 },
            size: crate::WindowSize {
                width: 640,
                height: 480,
            },
            state: "normal".to_string(),
            monitor_index: 0,
        };

        let outcome = restore_windows(&[target], false);
        let query = query_live_windows();
        let restored_hwnd = query.as_ref().ok().and_then(|result| {
            result
                .windows
                .iter()
                .find(|window| same_target(&window.path, &window.path_kind, &path, "filesystem"))
                .map(|window| window.hwnd)
        });
        if let Some(hwnd_raw) = restored_hwnd {
            let hwnd = HWND(hwnd_raw as *mut core::ffi::c_void);
            unsafe {
                let _ = PostMessageW(hwnd, WM_CLOSE, WPARAM(0), LPARAM(0));
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(300));
        let _ = std::fs::remove_dir(&folder);

        assert!(
            outcome.failed_items.is_empty(),
            "{:?}",
            outcome.failed_items
        );
        assert!(outcome.warnings.is_empty(), "{:?}", outcome.warnings);
        assert!(query.is_ok(), "restored window must be queryable");
        assert!(
            restored_hwnd.is_some(),
            "the exact temporary folder must be open"
        );
    }
}
