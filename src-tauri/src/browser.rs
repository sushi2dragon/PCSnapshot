//! Browser tab-URL capture.
//!
//! Primary path (all tabs): parse the browser's on-disk session file. Chromium
//! (Chrome/Edge/Brave) records every open window and tab — including inactive
//! ones — in an SNSS command log under `User Data\<profile>\Sessions\`. This is
//! the only way to recover *inactive* tab URLs: a background tab has no address
//! bar and no accessibility document, so it's invisible to UI Automation.
//!
//! Fallback path (active tab only): `read_active_tab_urls` walks each browser
//! window's UI Automation tree for the address bar. Used when the session file
//! can't be read (locked, missing, unknown browser).
//!
//! Everything is best-effort — any failure yields fewer URLs, never an error,
//! so capture is never blocked.

// ── Session-file path (all tabs, incl. inactive) ──────────────────────────────

/// Read every open tab URL for each given browser exe stem by parsing its newest
/// session file. Returns `(exe_stem, url)` pairs in tab order.
#[cfg(windows)]
pub fn read_open_tab_urls(stems: &std::collections::HashSet<String>) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for stem in stems {
        // Newest Session_ file across every candidate root/profile for this
        // browser (Opera GX vs Opera, multiple Chrome profiles, …).
        let session = browser_user_data_dirs(stem)
            .iter()
            .filter_map(|d| newest_session_file(d))
            .max_by_key(|(t, _)| *t)
            .map(|(_, p)| p);
        let Some(session) = session else {
            continue;
        };
        let Ok(data) = std::fs::read(&session) else {
            continue;
        };
        for url in parse_session_urls(&data) {
            out.push((stem.clone(), url));
        }
    }
    out
}

#[cfg(not(windows))]
pub fn read_open_tab_urls(_stems: &std::collections::HashSet<String>) -> Vec<(String, String)> {
    vec![]
}

/// Candidate Chromium `User Data` roots for a browser exe stem. Opera and Opera
/// GX share the `opera.exe` stem, so both are returned and the newest session
/// wins. Firefox uses a different (jsonlz4) format and is not handled here.
#[cfg(windows)]
fn browser_user_data_dirs(stem: &str) -> Vec<std::path::PathBuf> {
    use std::path::PathBuf;
    let local = std::env::var("LOCALAPPDATA").ok().map(PathBuf::from);
    let roaming = std::env::var("APPDATA").ok().map(PathBuf::from);
    let l = |sub: &str| local.as_ref().map(|b| b.join(sub));
    let r = |sub: &str| roaming.as_ref().map(|b| b.join(sub));
    let candidates: Vec<Option<PathBuf>> = match stem {
        "chrome" => vec![l(r"Google\Chrome\User Data")],
        "msedge" => vec![l(r"Microsoft\Edge\User Data")],
        "brave" => vec![l(r"BraveSoftware\Brave-Browser\User Data")],
        "vivaldi" => vec![l(r"Vivaldi\User Data")],
        "opera" | "opera_gx" => vec![
            r("Opera Software\\Opera GX Stable"),
            r("Opera Software\\Opera Stable"),
        ],
        _ => vec![],
    };
    candidates.into_iter().flatten().collect()
}

/// Newest `Session_*` file under a `User Data` root, checking each profile
/// subdirectory (`Default`, `Profile 1`, …) and the root itself. Returns its
/// modified time so callers can compare across candidate roots.
#[cfg(windows)]
fn newest_session_file(
    user_data: &std::path::Path,
) -> Option<(std::time::SystemTime, std::path::PathBuf)> {
    use std::time::SystemTime;
    let mut best: Option<(SystemTime, std::path::PathBuf)> = None;

    // Profile subdirectories plus the root (some browsers store Sessions there).
    let mut roots: Vec<std::path::PathBuf> = vec![user_data.to_path_buf()];
    if let Ok(entries) = std::fs::read_dir(user_data) {
        for e in entries.filter_map(|e| e.ok()) {
            if e.path().is_dir() {
                roots.push(e.path());
            }
        }
    }

    for root in roots {
        let Ok(files) = std::fs::read_dir(root.join("Sessions")) else {
            continue;
        };
        for f in files.filter_map(|e| e.ok()) {
            if !f.file_name().to_string_lossy().starts_with("Session_") {
                continue;
            }
            if let Ok(modified) = f.metadata().and_then(|m| m.modified()) {
                if best.as_ref().map_or(true, |(t, _)| modified > *t) {
                    best = Some((modified, f.path()));
                }
            }
        }
    }
    best
}

/// Parse a Chromium SNSS session file into the list of currently-open tab URLs,
/// ordered by (window, tab index). Reconstructs live state from the append-only
/// command log: tabs assigned to a window, minus closed tabs/windows, each
/// resolved to its selected navigation entry.
///
/// Command framing: 8-byte header ("SNSS" + i32 version), then repeated
/// `[u16 size][u8 id][payload size-1]`. Relevant commands (verified against real
/// files, Chromium session_service_commands.cc):
///   0  SetTabWindow            [i32 window_id, i32 tab_id]
///   2  SetTabIndexInWindow     [i32 tab_id, i32 index]
///   6  UpdateTabNavigation     [u32 pickle_len, i32 tab_id, i32 nav_index, str url, ...]
///   7  SetSelectedNavigationIdx[i32 tab_id, i32 nav_index]
///   16 TabClosed               [i32 tab_id, ...]
///   17 WindowClosed            [i32 window_id, ...]
#[cfg(windows)]
fn parse_session_urls(data: &[u8]) -> Vec<String> {
    use std::collections::{HashMap, HashSet};

    if data.len() < 8 || &data[0..4] != b"SNSS" {
        return vec![];
    }

    let i32_at = |b: &[u8], o: usize| -> Option<i32> {
        b.get(o..o + 4).map(|s| i32::from_le_bytes([s[0], s[1], s[2], s[3]]))
    };

    let mut tab_window: HashMap<i32, i32> = HashMap::new();
    let mut tab_index: HashMap<i32, i32> = HashMap::new();
    let mut selected_nav: HashMap<i32, i32> = HashMap::new();
    let mut navs: HashMap<i32, Vec<(i32, String)>> = HashMap::new();
    let mut closed_tabs: HashSet<i32> = HashSet::new();
    let mut closed_windows: HashSet<i32> = HashSet::new();
    let mut seen_tabs: Vec<i32> = Vec::new();

    let mut pos = 8usize;
    while pos + 2 <= data.len() {
        let size = u16::from_le_bytes([data[pos], data[pos + 1]]) as usize;
        pos += 2;
        if size == 0 || pos + size > data.len() {
            break;
        }
        let id = data[pos];
        let payload = &data[pos + 1..pos + size];
        pos += size;

        match id {
            0 => {
                if let (Some(w), Some(t)) = (i32_at(payload, 0), i32_at(payload, 4)) {
                    tab_window.insert(t, w);
                    if !seen_tabs.contains(&t) {
                        seen_tabs.push(t);
                    }
                }
            }
            2 => {
                if let (Some(t), Some(idx)) = (i32_at(payload, 0), i32_at(payload, 4)) {
                    tab_index.insert(t, idx);
                    if !seen_tabs.contains(&t) {
                        seen_tabs.push(t);
                    }
                }
            }
            6 => {
                // payload[0..4] = nested pickle size (ignored); fields follow.
                if let (Some(tab_id), Some(nav_index)) = (i32_at(payload, 4), i32_at(payload, 8)) {
                    if let Some(len) = payload
                        .get(12..16)
                        .map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]]) as usize)
                    {
                        if len <= 8192 {
                            if let Some(bytes) = payload.get(16..16 + len) {
                                if let Ok(url) = std::str::from_utf8(bytes) {
                                    if is_restorable_url(url) {
                                        navs.entry(tab_id).or_default().push((nav_index, url.to_string()));
                                    }
                                }
                            }
                        }
                    }
                }
            }
            7 => {
                if let (Some(t), Some(ni)) = (i32_at(payload, 0), i32_at(payload, 4)) {
                    selected_nav.insert(t, ni);
                }
            }
            16 => {
                if let Some(t) = i32_at(payload, 0) {
                    closed_tabs.insert(t);
                }
            }
            17 => {
                if let Some(w) = i32_at(payload, 0) {
                    closed_windows.insert(w);
                }
            }
            _ => {}
        }
    }

    // Surviving tabs, ordered by (window, tab index).
    let mut open: Vec<i32> = seen_tabs
        .into_iter()
        .filter(|t| {
            !closed_tabs.contains(t)
                && tab_window
                    .get(t)
                    .map_or(true, |w| !closed_windows.contains(w))
        })
        .collect();
    open.sort_by_key(|t| {
        (
            tab_window.get(t).copied().unwrap_or(i32::MAX),
            tab_index.get(t).copied().unwrap_or(i32::MAX),
        )
    });

    let mut urls = Vec::new();
    for t in open {
        let Some(entries) = navs.get(&t) else {
            continue;
        };
        // Prefer the selected navigation index; else the highest (most recent).
        let pick = selected_nav.get(&t).copied();
        let url = pick
            .and_then(|want| entries.iter().find(|(i, _)| *i == want).map(|(_, u)| u.clone()))
            .or_else(|| {
                entries
                    .iter()
                    .max_by_key(|(i, _)| *i)
                    .map(|(_, u)| u.clone())
            });
        if let Some(u) = url {
            urls.push(u);
        }
    }
    urls
}

/// Accept real navigable tabs; skip the new-tab page and internal chrome pages
/// that aren't worth reopening.
#[cfg(windows)]
fn is_restorable_url(url: &str) -> bool {
    let u = url.trim();
    if u.is_empty() {
        return false;
    }
    let low = u.to_ascii_lowercase();
    if low.starts_with("http://") || low.starts_with("https://") || low.starts_with("file://") {
        return true;
    }
    false
}

// ── UI Automation active-tab fallback ─────────────────────────────────────────

/// For each `(exe_stem, hwnd)` target, return `(exe_stem, url)` for the active
/// tab whose address bar we could read. Windows whose URL can't be read are
/// silently omitted. The whole sweep is bounded by a wall-clock deadline so it
/// can never threaten the <3s capture budget.
#[cfg(windows)]
pub fn read_active_tab_urls(targets: &[(String, isize)]) -> Vec<(String, String)> {
    use std::time::{Duration, Instant};
    use windows::Win32::Foundation::HWND;
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER,
        COINIT_APARTMENTTHREADED,
    };
    use windows::Win32::UI::Accessibility::{CUIAutomation, IUIAutomation};

    if targets.is_empty() {
        return vec![];
    }

    let mut out: Vec<(String, String)> = vec![];
    let deadline = Instant::now() + Duration::from_millis(2000);

    unsafe {
        // We may be on a thread Tauri already initialised; is_ok() is true only
        // when *we* added a COM reference, in which case we must balance it.
        let hr = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let owns_com = hr.is_ok();

        let automation: windows::core::Result<IUIAutomation> =
            CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER);

        if let Ok(automation) = automation {
            if let Ok(walker) = automation.RawViewWalker() {
                for (stem, hwnd_raw) in targets {
                    if Instant::now() >= deadline {
                        break;
                    }
                    let hwnd = HWND(*hwnd_raw as *mut core::ffi::c_void);
                    let Ok(root) = automation.ElementFromHandle(hwnd) else {
                        continue;
                    };
                    if let Some(url) = find_url(&walker, &root, deadline) {
                        out.push((stem.clone(), url));
                    }
                }
            }
        }

        if owns_com {
            CoUninitialize();
        }
    }

    out
}

/// Breadth-first search for the first Edit control holding a URL. BFS reaches the
/// shallow address bar (browser chrome) quickly without diving into the deep page
/// DOM first. Bounded by a node budget and the shared wall-clock deadline.
#[cfg(windows)]
unsafe fn find_url(
    walker: &windows::Win32::UI::Accessibility::IUIAutomationTreeWalker,
    root: &windows::Win32::UI::Accessibility::IUIAutomationElement,
    deadline: std::time::Instant,
) -> Option<String> {
    use std::collections::VecDeque;
    use std::time::Instant;
    use windows::core::Interface;
    use windows::Win32::UI::Accessibility::{
        IUIAutomationElement, IUIAutomationValuePattern, UIA_EditControlTypeId, UIA_ValuePatternId,
    };

    let mut budget: i32 = 800;
    let mut queue: VecDeque<(IUIAutomationElement, u32)> = VecDeque::new();
    queue.push_back((root.clone(), 0));

    while let Some((el, depth)) = queue.pop_front() {
        if budget <= 0 || Instant::now() >= deadline {
            break;
        }
        budget -= 1;

        // Is this an Edit control whose value looks like a URL? (The address bar.)
        if el.CurrentControlType().ok() == Some(UIA_EditControlTypeId) {
            if let Ok(unknown) = el.GetCurrentPattern(UIA_ValuePatternId) {
                if let Ok(vp) = unknown.cast::<IUIAutomationValuePattern>() {
                    if let Ok(bstr) = vp.CurrentValue() {
                        if let Some(url) = normalize_url(&bstr.to_string()) {
                            return Some(url);
                        }
                    }
                }
            }
        }

        // Enqueue children, but don't descend into the deep page content.
        if depth < 12 {
            let mut child = walker.GetFirstChildElement(&el).ok();
            while let Some(c) = child {
                let next = walker.GetNextSiblingElement(&c).ok();
                queue.push_back((c, depth + 1));
                child = next;
            }
        }
    }
    None
}

/// Accept values that look like a navigable address; reject empty boxes and
/// search queries. Address bars often elide the scheme, so re-add it.
#[cfg(windows)]
fn normalize_url(raw: &str) -> Option<String> {
    let s = raw.trim();
    if s.is_empty() || s.contains(' ') {
        return None; // search text / labels, not a URL
    }
    let lower = s.to_ascii_lowercase();
    if lower.starts_with("http://")
        || lower.starts_with("https://")
        || lower.starts_with("file://")
        || lower.starts_with("about:")
        || lower.contains("://")
    {
        return Some(s.to_string());
    }
    // Local dev servers are virtually always http.
    if lower.starts_with("localhost") || lower.starts_with("127.0.0.1") {
        return Some(format!("http://{s}"));
    }
    // Bare host like "github.com/x" → assume https.
    if s.contains('.') && !s.starts_with('.') {
        return Some(format!("https://{s}"));
    }
    None
}

#[cfg(not(windows))]
pub fn read_active_tab_urls(_targets: &[(String, isize)]) -> Vec<(String, String)> {
    vec![]
}

#[cfg(all(windows, test))]
mod tests {
    use super::parse_session_urls;

    /// Minimal SNSS builder for deterministic parser tests.
    struct Snss {
        buf: Vec<u8>,
    }
    impl Snss {
        fn new() -> Self {
            let mut buf = b"SNSS".to_vec();
            buf.extend_from_slice(&3i32.to_le_bytes()); // version
            Snss { buf }
        }
        fn cmd(&mut self, id: u8, payload: &[u8]) -> &mut Self {
            let size = (payload.len() + 1) as u16;
            self.buf.extend_from_slice(&size.to_le_bytes());
            self.buf.push(id);
            self.buf.extend_from_slice(payload);
            self
        }
        fn set_tab_window(&mut self, window: i32, tab: i32) -> &mut Self {
            let mut p = window.to_le_bytes().to_vec();
            p.extend_from_slice(&tab.to_le_bytes());
            self.cmd(0, &p)
        }
        fn set_tab_index(&mut self, tab: i32, index: i32) -> &mut Self {
            let mut p = tab.to_le_bytes().to_vec();
            p.extend_from_slice(&index.to_le_bytes());
            self.cmd(2, &p)
        }
        fn nav(&mut self, tab: i32, index: i32, url: &str) -> &mut Self {
            // [u32 pickle_size][i32 tab][i32 index][u32 url_len][url bytes]
            let mut inner = Vec::new();
            inner.extend_from_slice(&tab.to_le_bytes());
            inner.extend_from_slice(&index.to_le_bytes());
            inner.extend_from_slice(&(url.len() as u32).to_le_bytes());
            inner.extend_from_slice(url.as_bytes());
            let mut p = (inner.len() as u32).to_le_bytes().to_vec();
            p.extend_from_slice(&inner);
            self.cmd(6, &p)
        }
        fn selected_nav(&mut self, tab: i32, index: i32) -> &mut Self {
            let mut p = tab.to_le_bytes().to_vec();
            p.extend_from_slice(&index.to_le_bytes());
            self.cmd(7, &p)
        }
        fn tab_closed(&mut self, tab: i32) -> &mut Self {
            self.cmd(16, &tab.to_le_bytes())
        }
    }

    #[test]
    fn reconstructs_open_tabs_in_order() {
        let mut s = Snss::new();
        s.set_tab_window(100, 1).set_tab_index(1, 0)
            .set_tab_window(100, 2).set_tab_index(2, 1)
            .nav(1, 0, "https://a.com")
            .nav(2, 0, "https://b.com");
        assert_eq!(
            parse_session_urls(&s.buf),
            vec!["https://a.com".to_string(), "https://b.com".to_string()]
        );
    }

    #[test]
    fn excludes_closed_tabs() {
        let mut s = Snss::new();
        s.set_tab_window(100, 1).set_tab_index(1, 0).nav(1, 0, "https://keep.com")
            .set_tab_window(100, 2).set_tab_index(2, 1).nav(2, 0, "https://gone.com")
            .tab_closed(2);
        assert_eq!(parse_session_urls(&s.buf), vec!["https://keep.com".to_string()]);
    }

    #[test]
    fn uses_selected_navigation_not_history() {
        let mut s = Snss::new();
        // Tab navigated a→b→c, currently on b (index 1).
        s.set_tab_window(100, 1).set_tab_index(1, 0)
            .nav(1, 0, "https://a.com")
            .nav(1, 1, "https://b.com")
            .nav(1, 2, "https://c.com")
            .selected_nav(1, 1);
        assert_eq!(parse_session_urls(&s.buf), vec!["https://b.com".to_string()]);
    }

    #[test]
    fn falls_back_to_latest_nav_without_selected() {
        let mut s = Snss::new();
        s.set_tab_window(100, 1).set_tab_index(1, 0)
            .nav(1, 0, "https://old.com")
            .nav(1, 5, "https://current.com");
        assert_eq!(parse_session_urls(&s.buf), vec!["https://current.com".to_string()]);
    }

    #[test]
    fn rejects_non_snss() {
        assert!(parse_session_urls(b"not a session file").is_empty());
        assert!(parse_session_urls(&[]).is_empty());
    }

    /// Manual ground-truth check against the real browser session files on this
    /// machine, exercising the full stem→dir→newest-file→parse path. Ignored by
    /// default (machine-specific). Run with:
    ///   cargo test --lib real_browser_session -- --nocapture --ignored
    #[test]
    #[ignore]
    fn real_browser_session() {
        for stem in ["opera", "chrome", "msedge"] {
            let mut set = std::collections::HashSet::new();
            set.insert(stem.to_string());
            let urls = super::read_open_tab_urls(&set);
            eprintln!("=== {stem}: {} open tabs ===", urls.len());
            for (_, u) in &urls {
                eprintln!("  {u}");
            }
        }
    }
}
