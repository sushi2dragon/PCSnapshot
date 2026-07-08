//! Best-effort browser active-tab URL capture via UI Automation.
//!
//! Win32 window enumeration only exposes one HWND per browser *window*, so the
//! individual tabs are invisible to it. UI Automation, however, can read the
//! address bar of the *active* tab: we walk the element tree of each browser
//! window looking for the first Edit control whose value parses as a URL.
//!
//! Everything is strictly best-effort — any COM / UIA failure yields no URL
//! rather than an error, so capture is never blocked. The tree walk is bounded
//! in both depth and node count so a heavy page can't slow capture down.

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
