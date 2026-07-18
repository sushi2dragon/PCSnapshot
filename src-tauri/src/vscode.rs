use serde_json::Value;
use std::collections::HashSet;
use std::path::PathBuf;

/// Best-effort single-folder workspace paths from VS Code's periodically-flushed state.
/// A folder opened immediately before capture may not be present yet. Multi-root
/// `.code-workspace` entries are intentionally deferred.
pub fn open_folders(app_stem: &str) -> Vec<String> {
    let app_dir = match app_stem.to_ascii_lowercase().as_str() {
        "code" => "Code",
        "code-insiders" => "Code - Insiders",
        "cursor" => "Cursor",
        _ => return vec![],
    };
    let path = PathBuf::from(match std::env::var("APPDATA") {
        Ok(value) => value,
        Err(_) => return vec![],
    })
    .join(app_dir)
    .join("User")
    .join("globalStorage")
    .join("storage.json");
    let value: Value = match std::fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
    {
        Some(value) => value,
        None => return vec![],
    };

    let mut uris: Vec<&str> = vec![];
    if let Some(uri) = value
        .pointer("/windowsState/lastActiveWindow/folder")
        .and_then(Value::as_str)
    {
        uris.push(uri);
    }
    if let Some(windows) = value
        .pointer("/windowsState/openedWindows")
        .and_then(Value::as_array)
    {
        uris.extend(windows.iter().filter_map(|w| w.get("folder")?.as_str()));
    }
    if let Some(folders) = value
        .pointer("/backupWorkspaces/folders")
        .and_then(Value::as_array)
    {
        uris.extend(folders.iter().filter_map(|f| f.get("folderUri")?.as_str()));
    }

    let mut seen = HashSet::new();
    uris.into_iter()
        .filter_map(uri_to_path)
        .filter(|path| seen.insert(path.to_ascii_lowercase()))
        .collect()
}

fn uri_to_path(uri: &str) -> Option<String> {
    let raw = uri.strip_prefix("file://")?.trim_start_matches('/');
    let bytes = raw.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).ok()?;
            decoded.push(u8::from_str_radix(hex, 16).ok()?);
            i += 3;
        } else {
            decoded.push(bytes[i]);
            i += 1;
        }
    }
    let mut path = String::from_utf8(decoded).ok()?.replace('/', "\\");
    if path.len() < 3 || !path.as_bytes()[0].is_ascii_alphabetic() || &path[1..3] != ":\\" {
        return None;
    }
    path.replace_range(0..1, &path[0..1].to_ascii_uppercase());
    Some(path)
}

#[cfg(test)]
mod tests {
    use super::uri_to_path;

    #[test]
    fn decodes_drive_letter_spaces_and_nested_path() {
        assert_eq!(
            uri_to_path("file:///c%3A/Users/testuser/My%20Projects/PC%20Snapshot"),
            Some("C:\\Users\\testuser\\My Projects\\PC Snapshot".to_string())
        );
    }

    #[test]
    fn rejects_non_drive_file_uri() {
        assert_eq!(uri_to_path("file:///home/user/project"), None);
    }
}
