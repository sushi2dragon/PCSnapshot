//! Shared process classification.
//!
//! Maps an executable to a coarse category. Used by the capture engine to fill
//! `ProcessInfo.classification`, and by the restore engine to decide launch order
//! (background -> terminals -> IDEs -> browsers -> foreground) and which macros apply.
//!
//! Classification is a pure, allocation-light lookup on the lowercased exe file
//! stem so it costs effectively nothing during the speed-critical capture path.

/// Coarse process category. Ordered by restore-launch priority (see `launch_rank`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Category {
    Background,
    Terminal,
    Ide,
    Browser,
    Communication, // Teams, Slack, Discord, Zoom — launched after browsers
    Foreground,
    Other,
}

impl Category {
    /// String form persisted in the snapshot JSON (`ProcessInfo.classification`).
    pub fn as_str(self) -> &'static str {
        match self {
            Category::Background => "background",
            Category::Terminal => "terminal",
            Category::Ide => "ide",
            Category::Browser => "browser",
            Category::Communication => "communication",
            Category::Foreground => "foreground",
            Category::Other => "other",
        }
    }

    /// Parse back from the persisted string (used by restore). Unknown -> Other.
    pub fn from_str(s: &str) -> Category {
        match s {
            "background" => Category::Background,
            "terminal" => Category::Terminal,
            "ide" => Category::Ide,
            "browser" => Category::Browser,
            "communication" => Category::Communication,
            "foreground" => Category::Foreground,
            _ => Category::Other,
        }
    }

    /// Restore launch order: lower launches first.
    /// background -> terminals -> IDEs -> browsers -> communication -> foreground (last).
    pub fn launch_rank(self) -> u8 {
        match self {
            Category::Background => 0,
            Category::Terminal => 1,
            Category::Ide => 2,
            Category::Browser => 3,
            Category::Communication => 4,
            Category::Foreground => 5,
            Category::Other => 2, // treat unknown apps like mid-priority GUI apps
        }
    }

    pub fn is_browser(self) -> bool {
        matches!(self, Category::Browser)
    }

    pub fn is_communication(self) -> bool {
        matches!(self, Category::Communication)
    }
}

/// Lowercased exe file stem (no extension, no path) -> category.
/// `has_visible_window` lets us push windowless processes to `Background`.
pub fn classify(exe_path: &str, has_visible_window: bool) -> Category {
    let stem = exe_stem(exe_path);

    if is_browser(&stem) {
        return Category::Browser;
    }
    if is_ide(&stem) {
        return Category::Ide;
    }
    if is_terminal(&stem) {
        return Category::Terminal;
    }
    if is_communication(&stem) {
        return Category::Communication;
    }

    // No window of its own -> background helper / tray app / service.
    if !has_visible_window {
        return Category::Background;
    }

    Category::Other
}

/// Extract the lowercased file stem from a full path or bare name.
/// e.g. `C:\Program Files\Google\Chrome\Application\chrome.exe` -> `chrome`.
fn exe_stem(exe_path: &str) -> String {
    let last = exe_path
        .rsplit(|c| c == '\\' || c == '/')
        .next()
        .unwrap_or(exe_path);
    let stem = last.strip_suffix(".exe").or_else(|| last.strip_suffix(".EXE")).unwrap_or(last);
    stem.to_ascii_lowercase()
}

fn is_browser(stem: &str) -> bool {
    matches!(
        stem,
        "chrome" | "msedge" | "firefox" | "brave" | "opera" | "opera_gx" | "arc" | "vivaldi" | "chromium"
    )
}

fn is_ide(stem: &str) -> bool {
    // Versioned JetBrains launchers: the bare name or name + digits (idea64,
    // pycharm64, ...). Anything else after the prefix (e.g. "ideahelper") is
    // some unrelated tool and must not inherit IDE launch/arg handling.
    const JETBRAINS: [&str; 7] = ["idea", "pycharm", "webstorm", "clion", "goland", "rider", "rustrover"];
    if JETBRAINS.iter().any(|p| {
        stem.strip_prefix(p)
            .is_some_and(|rest| rest.is_empty() || rest.chars().all(|c| c.is_ascii_digit()))
    }) {
        return true;
    }
    matches!(
        stem,
        "code" | "code-insiders" | "cursor" | "devenv" | "sublime_text" | "fleet"
    )
}

fn is_terminal(stem: &str) -> bool {
    matches!(
        stem,
        "windowsterminal"
            | "wt"
            | "cmd"
            | "powershell"
            | "pwsh"
            | "conhost"
            | "alacritty"
            | "wezterm"
            | "wezterm-gui"
            | "mintty"
            | "tabby"
    )
}

fn is_communication(stem: &str) -> bool {
    // Teams: classic = "teams", new Teams = "ms-teams"
    // Also cover Slack, Discord, Zoom which have the same tray-resident behaviour.
    matches!(
        stem,
        "teams" | "ms-teams" | "slack" | "discord" | "zoom" | "webex" | "skype"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_known_apps() {
        assert_eq!(classify(r"C:\...\chrome.exe", true), Category::Browser);
        assert_eq!(classify(r"C:\...\Code.exe", true), Category::Ide);
        assert_eq!(classify(r"C:\...\idea64.exe", true), Category::Ide);
        assert_eq!(classify(r"C:\...\WindowsTerminal.exe", true), Category::Terminal);
        assert_eq!(classify(r"C:\...\helper.exe", false), Category::Background);
        assert_eq!(classify(r"C:\...\notepad.exe", true), Category::Other);
    }

    #[test]
    fn launch_order_is_foreground_last() {
        assert!(Category::Background.launch_rank() < Category::Browser.launch_rank());
        assert!(Category::Browser.launch_rank() < Category::Foreground.launch_rank());
    }
}
