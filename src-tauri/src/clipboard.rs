//! Clipboard capture & restore — the current clipboard plus the Windows Win+V
//! history.
//!
//! Governed entirely by the `capture_clipboard` opt-in (see `config.rs`): when
//! off, none of this runs. Supports **text and image** items only — Win+V
//! history never retains file copies, so they can never be reseeded and are out
//! of scope.
//!
//! Pinned Win+V items are preserved for free: `Clipboard::ClearHistory()` leaves
//! pinned items intact, and nothing here ever pins/unpins.
//!
//! ## Safety invariant (restore reseed)
//! `ClearHistory()` is destructive to the user's live history. It must never run
//! unless the current clipboard/history has first been captured, persisted
//! atomically, and verified. `reseed_history` enforces this: the caller passes a
//! `backup_ok` flag it only sets after a verified atomic backup, and the clear
//! is skipped otherwise.

use serde::{Deserialize, Serialize};
use std::path::Path;

/// Per-item byte cap — items larger than this are skipped (with a warning).
const MAX_ITEM_BYTES: u64 = 8 * 1024 * 1024; // 8 MiB
/// Total cap across a single capture.
const MAX_TOTAL_BYTES: u64 = 32 * 1024 * 1024; // 32 MiB
/// How long to wait for the OS to acknowledge one replayed write (the clipboard
/// sequence number moving) before giving up on that item.
#[cfg(windows)]
const REPLAY_CONFIRM_TIMEOUT: std::time::Duration = std::time::Duration::from_millis(500);
/// Gap after an acknowledged write, giving the history service — which consumes
/// clipboard changes asynchronously — time to file it as its own tile before
/// the next item overwrites the clipboard.
#[cfg(windows)]
const REPLAY_SETTLE: std::time::Duration = std::time::Duration::from_millis(300);

/// ## Why every WinRT call below is time-boxed
/// The Windows clipboard history service (cbdhsvc) can stop answering — most
/// reliably right after a `ClearHistory()` + rapid `SetContent` replay. Its
/// WinRT calls then block *forever*, with no error and no timeout of their own.
/// Because `take_snapshot` awaits clipboard capture, one wedged call used to
/// take the whole app down with it: capture, recapture and copy all hung and
/// nothing was ever written or logged.
///
/// So no clipboard call may block its caller. Each runs on its own detached
/// thread behind a bounded wait: a wedged service costs one leaked thread and
/// a warning, never the snapshot. Capture in particular always proceeds —
/// clipboard is an extra, not a prerequisite.
#[cfg(windows)]
fn bounded<T: Send + 'static>(
    what: &str,
    timeout: std::time::Duration,
    work: impl FnOnce() -> T + Send + 'static,
) -> Result<T, String> {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(work());
    });
    rx.recv_timeout(timeout).map_err(|_| {
        format!(
            "{what} timed out after {}s — the Windows clipboard history service is not responding",
            timeout.as_secs()
        )
    })
}

/// Ceiling for a full history read. Kept tight because capture has a sub-3s
/// budget: a healthy read is ~100ms, so this only ever bites when the service
/// is wedged, and the snapshot then saves without a clipboard block.
#[cfg(windows)]
const CAPTURE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(3);
/// Ceiling for setting one item on the clipboard.
#[cfg(windows)]
const COPY_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(4);
/// Ceiling for clearing Win+V history during a reseed.
#[cfg(windows)]
const CLEAR_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(3);
/// How long to wait for `ClearHistory` to actually empty the history (it
/// returns before the async wipe completes) before replaying on top anyway.
#[cfg(windows)]
const CLEAR_SETTLE_TIMEOUT: std::time::Duration = std::time::Duration::from_millis(1_500);

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ClipboardKind {
    Text,
    Image,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct ClipboardItem {
    pub id: String,
    pub kind: ClipboardKind,
    /// 0 = oldest; the highest `order` is the newest / top of the Win+V stack.
    pub order: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// Filename (not full path) of the PNG sidecar, for image items.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sidecar: Option<String>,
    /// "current" or "history".
    pub source: String,
    pub byte_size: u64,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct ClipboardBlock {
    pub captured_at: String,
    pub items: Vec<ClipboardItem>,
}

impl ClipboardBlock {
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}

// ---------------------------------------------------------------------------
// Non-Windows stubs so the crate still builds on other targets.
// ---------------------------------------------------------------------------

#[cfg(not(windows))]
pub fn capture(_dir: &Path, _id_prefix: &str) -> (Option<ClipboardBlock>, Vec<String>) {
    (None, vec!["Clipboard capture is only supported on Windows".into()])
}

#[cfg(not(windows))]
pub fn copy_item(_dir: &Path, _item: &ClipboardItem) -> Result<(), String> {
    Err("Clipboard copy is only supported on Windows".into())
}

#[cfg(not(windows))]
pub fn reseed_history(
    _dir: &Path,
    _block: &ClipboardBlock,
    _backup_ok: bool,
) -> Vec<String> {
    vec!["Clipboard reseed is only supported on Windows".into()]
}

// ---------------------------------------------------------------------------
// Windows implementation.
// ---------------------------------------------------------------------------

#[cfg(windows)]
mod win {
    use super::*;
    use windows::ApplicationModel::DataTransfer::{
        Clipboard, ClipboardHistoryItemsResultStatus, StandardDataFormats,
    };
    use windows::Storage::Streams::{DataReader, RandomAccessStreamReference};
    use windows::Win32::System::Com::{CoInitializeEx, COINIT_MULTITHREADED};

    /// Initialize the thread for WinRT/COM. Safe to call repeatedly; a benign
    /// "already initialized" / "changed mode" result is ignored.
    fn ensure_com() {
        unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        }
    }

    fn read_stream_bytes(
        stream_ref: &RandomAccessStreamReference,
    ) -> windows::core::Result<Vec<u8>> {
        let stream = stream_ref.OpenReadAsync()?.get()?;
        let size = stream.Size()?;
        if size == 0 {
            return Ok(Vec::new());
        }
        let reader = DataReader::CreateDataReader(&stream)?;
        reader.LoadAsync(size as u32)?.get()?;
        let mut buf = vec![0u8; size as usize];
        reader.ReadBytes(&mut buf)?;
        Ok(buf)
    }

    /// Convert raw clipboard bitmap bytes (PNG/BMP/DIB/etc.) to PNG bytes via the
    /// `image` crate. Falls back to the raw bytes if they already decode as an
    /// image but re-encoding fails, or returns None if undecodable.
    fn to_png(raw: &[u8]) -> Option<Vec<u8>> {
        match image::load_from_memory(raw) {
            Ok(img) => {
                let mut out = std::io::Cursor::new(Vec::new());
                match img.write_to(&mut out, image::ImageFormat::Png) {
                    Ok(()) => Some(out.into_inner()),
                    Err(_) => None,
                }
            }
            Err(_) => None,
        }
    }

    pub fn capture(dir: &Path, id_prefix: &str) -> (Option<ClipboardBlock>, Vec<String>) {
        ensure_com();
        let mut warnings = Vec::new();
        let mut items: Vec<ClipboardItem> = Vec::new();
        let mut total: u64 = 0;
        let mut next = 0u32;

        // --- Win+V history (primary source) ---
        let history_enabled = Clipboard::IsHistoryEnabled().unwrap_or(false);
        if history_enabled {
            match Clipboard::GetHistoryItemsAsync().and_then(|op| op.get()) {
                Ok(result) => {
                    let ok = result
                        .Status()
                        .map(|s| s == ClipboardHistoryItemsResultStatus::Success)
                        .unwrap_or(false);
                    if ok {
                        if let Ok(list) = result.Items() {
                            // WinRT returns newest-first; collect then reverse so
                            // `order` runs oldest -> newest.
                            let mut raw: Vec<ClipboardItem> = Vec::new();
                            for item in list {
                                let content = match item.Content() {
                                    Ok(c) => c,
                                    Err(_) => continue,
                                };
                                match extract(&content, dir, id_prefix, next, "history", &mut total)
                                {
                                    Ok(Some(ci)) => {
                                        next += 1;
                                        raw.push(ci);
                                    }
                                    Ok(None) => {}
                                    Err(e) => warnings.push(format!("Clipboard history item skipped: {e}")),
                                }
                                if total >= MAX_TOTAL_BYTES {
                                    warnings.push("Clipboard capture hit the total size cap".into());
                                    break;
                                }
                            }
                            raw.reverse();
                            for (i, mut ci) in raw.into_iter().enumerate() {
                                ci.order = i as u32;
                                items.push(ci);
                            }
                        }
                    } else {
                        warnings.push("Clipboard history unavailable (access denied or disabled)".into());
                    }
                }
                Err(e) => warnings.push(format!("Clipboard history read failed: {e}")),
            }
        } else {
            warnings.push("Win+V history is disabled; captured current clipboard only".into());
        }

        // --- Current clipboard (fallback when history was empty/disabled) ---
        // Sensitive-clipboard exclusion deferred to v1.1; capture unconditionally.
        if items.is_empty() {
            match Clipboard::GetContent() {
                Ok(content) => {
                    match extract(&content, dir, id_prefix, 0, "current", &mut total) {
                        Ok(Some(mut ci)) => {
                            ci.order = 0;
                            items.push(ci);
                        }
                        Ok(None) => {}
                        Err(e) => warnings.push(format!("Current clipboard skipped: {e}")),
                    }
                }
                Err(e) => warnings.push(format!("Current clipboard read failed: {e}")),
            }
        }

        if items.is_empty() {
            return (None, warnings);
        }
        let block = ClipboardBlock {
            captured_at: chrono::Utc::now().to_rfc3339(),
            items,
        };
        (Some(block), warnings)
    }

    /// Pull one item out of a DataPackageView. Returns Ok(None) when the content
    /// is neither text nor a usable image, or is too large.
    fn extract(
        content: &windows::ApplicationModel::DataTransfer::DataPackageView,
        dir: &Path,
        id_prefix: &str,
        index: u32,
        source: &str,
        total: &mut u64,
    ) -> Result<Option<ClipboardItem>, String> {
        let has_text = StandardDataFormats::Text()
            .and_then(|f| content.Contains(&f))
            .unwrap_or(false);
        let has_bitmap = StandardDataFormats::Bitmap()
            .and_then(|f| content.Contains(&f))
            .unwrap_or(false);

        if has_text {
            let text: String = content
                .GetTextAsync()
                .and_then(|op| op.get())
                .map(|h| h.to_string_lossy())
                .map_err(|e| format!("{e}"))?;
            let size = text.len() as u64;
            if size > MAX_ITEM_BYTES {
                return Ok(None);
            }
            *total += size;
            return Ok(Some(ClipboardItem {
                id: format!("clip_{index}"),
                kind: ClipboardKind::Text,
                order: index,
                text: Some(text),
                sidecar: None,
                source: source.to_string(),
                byte_size: size,
            }));
        }

        if has_bitmap {
            let stream_ref = content
                .GetBitmapAsync()
                .and_then(|op| op.get())
                .map_err(|e| format!("{e}"))?;
            let raw = read_stream_bytes(&stream_ref).map_err(|e| format!("{e}"))?;
            if raw.is_empty() {
                return Ok(None);
            }
            let png = match to_png(&raw) {
                Some(p) => p,
                None => return Ok(None),
            };
            let size = png.len() as u64;
            if size > MAX_ITEM_BYTES {
                return Ok(None);
            }
            let filename = format!("{id_prefix}_clip_{index}.png");
            let path = dir.join(&filename);
            std::fs::write(&path, &png).map_err(|e| format!("sidecar write: {e}"))?;
            *total += size;
            return Ok(Some(ClipboardItem {
                id: format!("clip_{index}"),
                kind: ClipboardKind::Image,
                order: index,
                text: None,
                sidecar: Some(filename),
                source: source.to_string(),
                byte_size: size,
            }));
        }

        Ok(None)
    }

    /// Number of items currently in the Win+V history, or None if it can't be
    /// read. Cheap — it counts the result list without touching item content.
    pub fn history_len() -> Option<usize> {
        ensure_com();
        let result = Clipboard::GetHistoryItemsAsync().and_then(|op| op.get()).ok()?;
        let ok = result
            .Status()
            .map(|s| s == ClipboardHistoryItemsResultStatus::Success)
            .unwrap_or(false);
        if !ok {
            return None;
        }
        result.Items().ok().and_then(|list| list.Size().ok()).map(|n| n as usize)
    }

    /// Clearing unpinned Win+V history (pinned items survive) is the one part of
    /// a reseed with no Win32 equivalent, so it must go through WinRT. Putting
    /// the items *back* does not — see `reseed_history`.
    ///
    /// `ClearHistory()` returns before the service has finished wiping, and
    /// replayed items that arrive during that window are swept away with the old
    /// ones — which is why the first restore after a clear used to put nothing
    /// back and only a second press worked. So we poll until the history is
    /// actually empty (bounded) before returning.
    pub fn clear_history() -> Result<(), String> {
        ensure_com();
        if !Clipboard::IsHistoryEnabled().unwrap_or(false) {
            return Err("Win+V history is disabled".into());
        }
        // ClearHistory reports refusal as Ok(false), not as an error.
        match Clipboard::ClearHistory() {
            Ok(true) => {}
            Ok(false) => return Err("Windows declined to clear the history".into()),
            Err(e) => return Err(format!("{e}")),
        }

        let deadline = std::time::Instant::now() + CLEAR_SETTLE_TIMEOUT;
        loop {
            match history_len() {
                Some(0) => return Ok(()),
                _ if std::time::Instant::now() >= deadline => return Ok(()),
                _ => std::thread::sleep(std::time::Duration::from_millis(25)),
            }
        }
    }
}

#[cfg(windows)]
pub fn capture(dir: &Path, id_prefix: &str) -> (Option<ClipboardBlock>, Vec<String>) {
    // Read the current item over plain Win32 first. It costs milliseconds and,
    // unlike anything WinRT, it keeps working when the history service is down —
    // so this is the floor: a snapshot always records what the user has copied,
    // even if the Win+V history can't be read at all.
    let (current, mut warnings) = win32::capture_current(dir, id_prefix);

    let owned_dir = dir.to_path_buf();
    let prefix = id_prefix.to_string();
    let history = bounded("Clipboard capture", CAPTURE_TIMEOUT, move || {
        win::capture(&owned_dir, &prefix)
    });

    let (history_block, history_warnings) = match history {
        Ok((block, w)) => (block, w),
        Err(e) => (None, vec![format!("{e}; captured the current clipboard only")]),
    };
    warnings.extend(history_warnings);

    match history_block {
        Some(mut block) => {
            // History normally carries the current item as its newest entry —
            // but the service files new items asynchronously, so a read taken
            // moments after a copy can miss the very thing the user just
            // copied. Keep the Win32 read in that case rather than lose it.
            let newest = block.items.iter().max_by_key(|item| item.order);
            let represented = match (&current, newest) {
                (None, _) => true,
                (Some(_), None) => false,
                (Some(cur), Some(newest)) => match cur.kind {
                    ClipboardKind::Text => {
                        newest.kind == ClipboardKind::Text && newest.text == cur.text
                    }
                    // The two paths re-encode PNGs differently, so bytes can't
                    // be compared; a newest-is-an-image entry is the current
                    // clipboard image.
                    ClipboardKind::Image => newest.kind == ClipboardKind::Image,
                },
            };

            match current {
                Some(item) if !represented => {
                    let order = block.items.iter().map(|i| i.order).max().unwrap_or(0) + 1;
                    block.items.push(ClipboardItem {
                        id: format!("clip_{order}"),
                        order,
                        ..item
                    });
                }
                other => {
                    if let Some(name) = other.and_then(|i| i.sidecar) {
                        let _ = std::fs::remove_file(dir.join(name));
                    }
                }
            }
            (Some(block), warnings)
        }
        None => match current {
            Some(item) => (
                Some(ClipboardBlock {
                    captured_at: chrono::Utc::now().to_rfc3339(),
                    items: vec![item],
                }),
                warnings,
            ),
            None => (None, warnings),
        },
    }
}

#[cfg(windows)]
pub fn copy_item(dir: &Path, item: &ClipboardItem) -> Result<(), String> {
    let dir = dir.to_path_buf();
    let item = item.clone();
    bounded("Clipboard copy", COPY_TIMEOUT, move || {
        win32::copy_item(&dir, &item)
    })?
}

// ---------------------------------------------------------------------------
// Win32 copy path — deliberately not WinRT.
// ---------------------------------------------------------------------------

/// `Clipboard::SetContent` is a WinRT call, so it rides on the clipboard
/// history service (cbdhsvc). When that service stops answering, every WinRT
/// clipboard call blocks — while the plain Win32 clipboard keeps working
/// normally (other apps copy and paste fine throughout).
///
/// Re-copying a stored item is the one clipboard feature with no reason to
/// depend on history at all, so it goes through Win32 directly and keeps
/// working even while `capture` is timing out.
#[cfg(windows)]
mod win32 {
    use super::*;
    use windows::Win32::Foundation::{HANDLE, HWND};
    use windows::Win32::Foundation::HGLOBAL;
    use windows::Win32::System::DataExchange::{
        CloseClipboard, EmptyClipboard, GetClipboardData, GetClipboardSequenceNumber,
        IsClipboardFormatAvailable, OpenClipboard, RegisterClipboardFormatW, SetClipboardData,
    };
    use windows::Win32::System::Memory::{
        GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock, GMEM_MOVEABLE,
    };

    const CF_UNICODETEXT: u32 = 13;
    const CF_DIB: u32 = 8;

    /// Serializes this process's Win32 clipboard access.
    ///
    /// Within a single process `GetClipboardData` can hand back the very
    /// `HGLOBAL` we placed there, so a concurrent `EmptyClipboard` on another
    /// thread frees memory a reader still has locked — which shows up as a
    /// heap-corruption crash, not an error code. `OpenClipboard` guards against
    /// *other processes*, not against ourselves, so we provide the lock.
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn guard() -> std::sync::MutexGuard<'static, ()> {
        LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// True if the current clipboard carries an "exclude from history / monitor
    /// processing" marker (set by password managers). Pure Win32 — no history
    /// service involved.
    ///
    /// DEFERRED (v1.1): not currently called — the sensitive-clipboard exclusion
    /// was dropped for the local, network-less current release. Kept so
    /// re-enabling is a one-line change at the two former call sites
    /// (`capture_current` and `win::capture`). Note: before re-wiring, fix the
    /// `CanIncludeInClipboardHistory` check to read the marker's DWORD value
    /// (0 = exclude) rather than treating mere presence as sensitive, which
    /// false-positives on Chrome/Edge/Office copies. See
    /// features/pending/clipboard_restore.md.
    #[allow(dead_code)]
    pub fn current_is_sensitive() -> bool {
        let names = [
            "ExcludeClipboardContentFromMonitorProcessing",
            "CanIncludeInClipboardHistory",
        ];
        for name in names {
            let wide: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();
            let fmt = unsafe { RegisterClipboardFormatW(windows::core::PCWSTR(wide.as_ptr())) };
            if fmt != 0 && unsafe { IsClipboardFormatAvailable(fmt) }.is_ok() {
                return true;
            }
        }
        false
    }

    /// Moveable global block filled with `bytes`. On success the clipboard owns
    /// the handle, so it is never freed here.
    unsafe fn global_from(bytes: &[u8]) -> Result<HANDLE, String> {
        let handle = GlobalAlloc(GMEM_MOVEABLE, bytes.len()).map_err(|e| format!("GlobalAlloc: {e}"))?;
        let ptr = GlobalLock(handle);
        if ptr.is_null() {
            return Err("GlobalLock returned null".into());
        }
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), ptr as *mut u8, bytes.len());
        let _ = GlobalUnlock(handle);
        Ok(HANDLE(handle.0))
    }

    /// The clipboard is one system-wide lock and another app may hold it for a
    /// moment; retry briefly rather than failing the user's click.
    unsafe fn open_clipboard() -> Result<(), String> {
        for attempt in 0..10u64 {
            if OpenClipboard(HWND(std::ptr::null_mut())).is_ok() {
                return Ok(());
            }
            std::thread::sleep(std::time::Duration::from_millis(20 * (attempt + 1)));
        }
        Err("another application is holding the clipboard open".into())
    }

    fn set_formats(entries: &[(u32, Vec<u8>)]) -> Result<(), String> {
        let _guard = guard();
        unsafe {
            open_clipboard()?;
            let result = (|| {
                EmptyClipboard().map_err(|e| format!("EmptyClipboard: {e}"))?;
                for (format, bytes) in entries {
                    let handle = global_from(bytes)?;
                    SetClipboardData(*format, handle).map_err(|e| format!("SetClipboardData: {e}"))?;
                }
                Ok(())
            })();
            let _ = CloseClipboard();
            result
        }
    }

    fn text_bytes(text: &str) -> Vec<u8> {
        text.encode_utf16()
            .chain(std::iter::once(0))
            .flat_map(|unit| unit.to_le_bytes())
            .collect()
    }

    /// CF_DIB payload: BITMAPINFOHEADER followed by bottom-up BGRA rows. 32bpp
    /// BI_RGB is the widest-compatibility form; consumers that ignore the alpha
    /// byte paste it opaque, which is how Windows pastes screenshots anyway.
    fn dib_bytes(png: &[u8]) -> Result<Vec<u8>, String> {
        let img = image::load_from_memory(png)
            .map_err(|e| format!("image decode: {e}"))?
            .to_rgba8();
        let (width, height) = img.dimensions();
        let pixel_bytes = (width as usize) * (height as usize) * 4;
        let mut out = Vec::with_capacity(40 + pixel_bytes);
        out.extend_from_slice(&40u32.to_le_bytes()); // biSize
        out.extend_from_slice(&(width as i32).to_le_bytes());
        out.extend_from_slice(&(height as i32).to_le_bytes()); // positive = bottom-up
        out.extend_from_slice(&1u16.to_le_bytes()); // biPlanes
        out.extend_from_slice(&32u16.to_le_bytes()); // biBitCount
        out.extend_from_slice(&0u32.to_le_bytes()); // biCompression = BI_RGB
        out.extend_from_slice(&(pixel_bytes as u32).to_le_bytes());
        out.extend_from_slice(&0i32.to_le_bytes()); // biXPelsPerMeter
        out.extend_from_slice(&0i32.to_le_bytes()); // biYPelsPerMeter
        out.extend_from_slice(&0u32.to_le_bytes()); // biClrUsed
        out.extend_from_slice(&0u32.to_le_bytes()); // biClrImportant

        let raw = img.as_raw();
        let stride = (width as usize) * 4;
        for y in (0..height as usize).rev() {
            for px in raw[y * stride..y * stride + stride].chunks_exact(4) {
                out.extend_from_slice(&[px[2], px[1], px[0], px[3]]);
            }
        }
        Ok(out)
    }

    /// The OS clipboard sequence number, bumped on every clipboard change.
    /// Needs no open handle, so it is safe to poll between writes.
    pub fn sequence_number() -> u32 {
        unsafe { GetClipboardSequenceNumber() }
    }

    /// Copy the bytes behind a clipboard format handle. Returns None when the
    /// format isn't present.
    unsafe fn read_format(format: u32) -> Option<Vec<u8>> {
        if IsClipboardFormatAvailable(format).is_err() {
            return None;
        }
        let handle = GetClipboardData(format).ok()?;
        let hglobal = HGLOBAL(handle.0);
        let size = GlobalSize(hglobal);
        if size == 0 {
            return None;
        }
        let ptr = GlobalLock(hglobal);
        if ptr.is_null() {
            return None;
        }
        let mut bytes = vec![0u8; size];
        std::ptr::copy_nonoverlapping(ptr as *const u8, bytes.as_mut_ptr(), size);
        let _ = GlobalUnlock(hglobal);
        Some(bytes)
    }

    const BI_RGB: u32 = 0;
    const BI_BITFIELDS: u32 = 3;

    /// Scale a channel value extracted through `mask` up to 8 bits. Handles any
    /// mask width, so a 5-bit (16bpp) or 8-bit (32bpp) channel both map to 0–255.
    fn channel8(value: u32, mask: u32) -> u8 {
        if mask == 0 {
            return 0;
        }
        let shift = mask.trailing_zeros();
        let width = mask.count_ones();
        let raw = (value & mask) >> shift;
        let max = (1u32 << width) - 1;
        ((raw * 255 + max / 2) / max) as u8
    }

    /// Decode a CF_DIB payload (BITMAPINFOHEADER + pixel rows) to PNG bytes.
    ///
    /// Handles both BI_RGB and BI_BITFIELDS — the latter (compression 3) is what
    /// Windows uses for most 32bpp clipboard images, and rejecting it was why a
    /// screenshot on the clipboard came back "unsupported DIB". Channels are
    /// read through the bitfield masks rather than assumed to be in BGRA byte
    /// order. Alpha is forced opaque: clipboard bitmaps are opaque, and an
    /// all-zero alpha mask would otherwise yield a fully transparent PNG.
    pub fn dib_to_png(dib: &[u8]) -> Result<Vec<u8>, String> {
        if dib.len() < 40 {
            return Err("DIB header truncated".into());
        }
        let u32_at = |o: usize| u32::from_le_bytes([dib[o], dib[o + 1], dib[o + 2], dib[o + 3]]);
        let i32_at = |o: usize| i32::from_le_bytes([dib[o], dib[o + 1], dib[o + 2], dib[o + 3]]);
        let header_size = u32_at(0) as usize;
        let width = i32_at(4);
        let height = i32_at(8);
        let bit_count = u16::from_le_bytes([dib[14], dib[15]]);
        let compression = u32_at(16);
        let palette_entries = u32_at(32) as usize;

        if !(bit_count == 24 || bit_count == 32) || !(compression == BI_RGB || compression == BI_BITFIELDS) {
            return Err(format!(
                "unsupported DIB (compression {compression}, {bit_count}bpp)"
            ));
        }
        if width <= 0 || height == 0 {
            return Err("DIB has no pixels".into());
        }

        // Channel masks. For BI_BITFIELDS they follow a 40-byte header (or live
        // inside a V4/V5 header at the same offsets); for BI_RGB the layout is
        // the fixed BGR(A) order.
        let (r_mask, g_mask, b_mask) = if compression == BI_BITFIELDS {
            if dib.len() < header_size.max(52) {
                return Err("DIB bitfield masks truncated".into());
            }
            (u32_at(40), u32_at(44), u32_at(48))
        } else {
            // BI_RGB stores BGR(A) in memory: blue in the low byte, red high.
            (0x00FF_0000, 0x0000_FF00, 0x0000_00FF)
        };

        let bottom_up = height > 0;
        let (width, height) = (width as usize, height.unsigned_abs() as usize);
        let bytes_per_px = (bit_count / 8) as usize;
        let stride = ((width * bytes_per_px + 3) / 4) * 4;
        // BI_BITFIELDS with a 40-byte header inserts 3 DWORD masks before pixels.
        let mask_block = if compression == BI_BITFIELDS && header_size == 40 { 12 } else { 0 };
        let offset = header_size + mask_block + palette_entries * 4;
        if dib.len() < offset + stride * height {
            return Err("DIB pixel data truncated".into());
        }

        let mut rgba = Vec::with_capacity(width * height * 4);
        for row in 0..height {
            let src_row = if bottom_up { height - 1 - row } else { row };
            let start = offset + src_row * stride;
            for px in dib[start..start + width * bytes_per_px].chunks_exact(bytes_per_px) {
                let value = if bytes_per_px == 4 {
                    u32::from_le_bytes([px[0], px[1], px[2], px[3]])
                } else {
                    u32::from_le_bytes([px[0], px[1], px[2], 0])
                };
                rgba.push(channel8(value, r_mask));
                rgba.push(channel8(value, g_mask));
                rgba.push(channel8(value, b_mask));
                rgba.push(255);
            }
        }

        let buffer = image::RgbaImage::from_raw(width as u32, height as u32, rgba)
            .ok_or_else(|| "DIB pixel buffer size mismatch".to_string())?;
        let mut png = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(buffer)
            .write_to(&mut png, image::ImageFormat::Png)
            .map_err(|e| format!("PNG encode: {e}"))?;
        Ok(png.into_inner())
    }

    /// Read whatever is on the clipboard right now, without touching the history
    /// service. This is the floor the capture pipeline falls back to: even with
    /// cbdhsvc wedged, a snapshot still records the item the user has copied.
    pub fn capture_current(dir: &Path, id_prefix: &str) -> (Option<ClipboardItem>, Vec<String>) {
        // Sensitive-clipboard exclusion is deferred to v1.1: data is stored
        // locally and the app has no network access, so the current release
        // captures the live clipboard unconditionally. `current_is_sensitive`
        // is retained (unwired) for the re-enable — see
        // features/pending/clipboard_restore.md.
        let _guard = guard();
        unsafe {
            if let Err(e) = open_clipboard() {
                return (None, vec![format!("Current clipboard not read: {e}")]);
            }
            let text = read_format(CF_UNICODETEXT);
            let dib = if text.is_none() { read_format(CF_DIB) } else { None };
            let _ = CloseClipboard();

            if let Some(bytes) = text {
                let units: Vec<u16> = bytes
                    .chunks_exact(2)
                    .map(|c| u16::from_le_bytes([c[0], c[1]]))
                    .take_while(|u| *u != 0)
                    .collect();
                let text = String::from_utf16_lossy(&units);
                if text.is_empty() {
                    return (None, Vec::new());
                }
                let size = text.len() as u64;
                if size > MAX_ITEM_BYTES {
                    return (None, vec!["Current clipboard item exceeds the size cap".into()]);
                }
                return (
                    Some(ClipboardItem {
                        id: "clip_0".into(),
                        kind: ClipboardKind::Text,
                        order: 0,
                        text: Some(text),
                        sidecar: None,
                        source: "current".into(),
                        byte_size: size,
                    }),
                    Vec::new(),
                );
            }

            if let Some(bytes) = dib {
                let png = match dib_to_png(&bytes) {
                    Ok(p) => p,
                    Err(e) => return (None, vec![format!("Current clipboard image skipped: {e}")]),
                };
                let size = png.len() as u64;
                if size > MAX_ITEM_BYTES {
                    return (None, vec!["Current clipboard image exceeds the size cap".into()]);
                }
                let filename = format!("{id_prefix}_clip_current.png");
                if let Err(e) = std::fs::write(dir.join(&filename), &png) {
                    return (None, vec![format!("Clipboard sidecar write failed: {e}")]);
                }
                return (
                    Some(ClipboardItem {
                        id: "clip_0".into(),
                        kind: ClipboardKind::Image,
                        order: 0,
                        text: None,
                        sidecar: Some(filename),
                        source: "current".into(),
                        byte_size: size,
                    }),
                    Vec::new(),
                );
            }
        }
        (None, Vec::new())
    }

    pub fn copy_item(dir: &Path, item: &ClipboardItem) -> Result<(), String> {
        match item.kind {
            ClipboardKind::Text => set_formats(&[(
                CF_UNICODETEXT,
                text_bytes(item.text.as_deref().unwrap_or("")),
            )]),
            ClipboardKind::Image => {
                let filename = item
                    .sidecar
                    .as_ref()
                    .ok_or_else(|| "image item has no sidecar".to_string())?;
                let path = dir.join(filename);
                let png = std::fs::read(&path).map_err(|e| format!("sidecar read: {e}"))?;
                set_formats(&[(CF_DIB, dib_bytes(&png)?)])
            }
        }
    }
}

#[cfg(all(windows, test))]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn bounded_passes_the_value_through_when_work_finishes_in_time() {
        assert_eq!(bounded("probe", Duration::from_secs(5), || 7), Ok(7));
    }

    /// A hand-built 2x2 32bpp BI_BITFIELDS DIB — the format that was rejected as
    /// "unsupported DIB (compression 3, 32bpp)". Masks are the standard BGRA
    /// layout; decoding must recover the exact colours regardless of byte order.
    #[test]
    fn decodes_a_32bpp_bitfields_dib() {
        let mut dib = Vec::new();
        dib.extend_from_slice(&40u32.to_le_bytes()); // biSize
        dib.extend_from_slice(&2i32.to_le_bytes()); // width
        dib.extend_from_slice(&2i32.to_le_bytes()); // height (bottom-up)
        dib.extend_from_slice(&1u16.to_le_bytes()); // planes
        dib.extend_from_slice(&32u16.to_le_bytes()); // bpp
        dib.extend_from_slice(&3u32.to_le_bytes()); // BI_BITFIELDS
        dib.extend_from_slice(&16u32.to_le_bytes()); // biSizeImage
        dib.extend_from_slice(&0i32.to_le_bytes());
        dib.extend_from_slice(&0i32.to_le_bytes());
        dib.extend_from_slice(&0u32.to_le_bytes());
        dib.extend_from_slice(&0u32.to_le_bytes());
        dib.extend_from_slice(&0x00FF_0000u32.to_le_bytes()); // red mask
        dib.extend_from_slice(&0x0000_FF00u32.to_le_bytes()); // green mask
        dib.extend_from_slice(&0x0000_00FFu32.to_le_bytes()); // blue mask
        // Pixels are stored B,G,R,X. Bottom-up: first row here is the image's
        // bottom row. Bottom-left red, bottom-right green, top-left blue, top-right white.
        let px = |b: u8, g: u8, r: u8| [b, g, r, 0u8];
        dib.extend_from_slice(&px(0, 0, 255)); // red
        dib.extend_from_slice(&px(0, 255, 0)); // green
        dib.extend_from_slice(&px(255, 0, 0)); // blue
        dib.extend_from_slice(&px(255, 255, 255)); // white

        let png = win32::dib_to_png(&dib).expect("bitfields DIB should decode");
        let img = image::load_from_memory(&png).expect("re-decode png").to_rgba8();
        assert_eq!(img.dimensions(), (2, 2));
        assert_eq!(img.get_pixel(0, 0).0, [0, 0, 255, 255]); // top-left blue
        assert_eq!(img.get_pixel(1, 0).0, [255, 255, 255, 255]); // top-right white
        assert_eq!(img.get_pixel(0, 1).0, [255, 0, 0, 255]); // bottom-left red
        assert_eq!(img.get_pixel(1, 1).0, [0, 255, 0, 255]); // bottom-right green
    }

    // The three tests below drive the real system clipboard, so they are
    // #[ignore]d: they clobber whatever the developer had copied. Run with
    // `cargo test --lib win32 -- --ignored`.

    /// The clipboard is one global resource. The module's own lock keeps
    /// concurrent access memory-safe, but it cannot make a set-then-read pair
    /// atomic — without this, one test's write lands between another's copy and
    /// read-back.
    static CLIPBOARD_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn exclusive() -> std::sync::MutexGuard<'static, ()> {
        CLIPBOARD_TEST_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Leaves a sentinel on the live clipboard for an external reader to confirm.
    #[test]
    #[ignore]
    fn win32_copy_puts_text_on_the_live_clipboard() {
        let _serial = exclusive();
        let item = ClipboardItem {
            id: "probe".into(),
            kind: ClipboardKind::Text,
            order: 0,
            text: Some("PC-SNAPSHOT-WIN32-PROBE".into()),
            sidecar: None,
            source: "current".into(),
            byte_size: 0,
        };
        win32::copy_item(&std::env::temp_dir(), &item).expect("win32 copy failed");
    }

    #[test]
    #[ignore]
    fn win32_round_trips_text_through_the_live_clipboard() {
        let _serial = exclusive();
        let dir = std::env::temp_dir();
        let item = ClipboardItem {
            id: "probe".into(),
            kind: ClipboardKind::Text,
            order: 0,
            text: Some("PC-SNAPSHOT-ROUNDTRIP".into()),
            sidecar: None,
            source: "current".into(),
            byte_size: 0,
        };
        win32::copy_item(&dir, &item).expect("win32 copy failed");

        let (read, warnings) = win32::capture_current(&dir, "probe_rt");
        let read = read.unwrap_or_else(|| panic!("nothing captured: {warnings:?}"));
        assert_eq!(read.kind, ClipboardKind::Text);
        assert_eq!(read.text.as_deref(), Some("PC-SNAPSHOT-ROUNDTRIP"));
    }

    /// Exercises both halves of the image path — PNG→CF_DIB on the way out and
    /// CF_DIB→PNG on the way back — including BGRA channel order.
    #[test]
    #[ignore]
    fn win32_round_trips_an_image_through_the_live_clipboard() {
        let _serial = exclusive();
        let dir = std::env::temp_dir();
        let source = image::RgbaImage::from_fn(3, 2, |x, y| {
            image::Rgba([(x as u8) * 80, (y as u8) * 120, 200, 255])
        });
        let mut png = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(source.clone())
            .write_to(&mut png, image::ImageFormat::Png)
            .expect("encode source png");
        std::fs::write(dir.join("probe_src.png"), png.into_inner()).expect("write source png");

        let item = ClipboardItem {
            id: "probe".into(),
            kind: ClipboardKind::Image,
            order: 0,
            text: None,
            sidecar: Some("probe_src.png".into()),
            source: "current".into(),
            byte_size: 0,
        };
        win32::copy_item(&dir, &item).expect("win32 image copy failed");

        let (read, warnings) = win32::capture_current(&dir, "probe_rt");
        let read = read.unwrap_or_else(|| panic!("nothing captured: {warnings:?}"));
        assert_eq!(read.kind, ClipboardKind::Image);
        let sidecar = read.sidecar.expect("image item has a sidecar");
        let bytes = std::fs::read(dir.join(sidecar)).expect("read captured sidecar");
        let decoded = image::load_from_memory(&bytes).expect("sidecar decodes").to_rgba8();
        assert_eq!(decoded.dimensions(), (3, 2));
        assert_eq!(decoded.get_pixel(0, 0).0, source.get_pixel(0, 0).0);
        assert_eq!(decoded.get_pixel(2, 1).0, source.get_pixel(2, 1).0);
    }

    /// Diagnostic: replays five images of increasing size, then reads the Win+V
    /// history back and reports how many of them the service actually filed.
    /// Never clears history. Prints rather than asserts — it measures the
    /// service's behaviour, which is what the size-scaled settle is tuned to.
    #[test]
    #[ignore]
    fn diagnostic_how_many_replayed_images_reach_the_history() {
        let _serial = exclusive();
        let dir = std::env::temp_dir();
        let dimensions = [(8u32, 8u32), (64, 64), (256, 256), (512, 512), (900, 900)];

        let mut items = Vec::new();
        for (index, (width, height)) in dimensions.iter().enumerate() {
            let img = image::RgbaImage::from_fn(*width, *height, |x, y| {
                image::Rgba([(x % 256) as u8, (y % 256) as u8, (index as u8) * 40, 255])
            });
            let mut png = std::io::Cursor::new(Vec::new());
            image::DynamicImage::ImageRgba8(img)
                .write_to(&mut png, image::ImageFormat::Png)
                .expect("encode probe png");
            let png = png.into_inner();
            let name = format!("probe_img_{index}.png");
            std::fs::write(dir.join(&name), &png).expect("write probe png");
            items.push(ClipboardItem {
                id: format!("probe_{index}"),
                kind: ClipboardKind::Image,
                order: index as u32,
                text: None,
                sidecar: Some(name),
                source: "current".into(),
                byte_size: png.len() as u64,
            });
        }

        for item in &items {
            let acknowledged = replay_one(&dir, item).expect("replay failed");
            println!(
                "replayed {}x{} ({} KB) acknowledged={acknowledged}",
                dimensions[item.order as usize].0,
                dimensions[item.order as usize].1,
                item.byte_size / 1024
            );
        }

        std::thread::sleep(std::time::Duration::from_millis(1200));
        let (block, warnings) = win::capture(&dir, "probe_hist");
        println!("history read warnings: {warnings:?}");
        match block {
            Some(block) => {
                let images = block
                    .items
                    .iter()
                    .filter(|i| i.kind == ClipboardKind::Image)
                    .count();
                println!(
                    "history now holds {} items, {images} of them images (replayed 5)",
                    block.items.len()
                );
            }
            None => println!("history read returned nothing"),
        }
    }

    /// Exercises the real replay step used by a reseed, five items deep. It
    /// never clears history, so it is safe to run against a live machine.
    /// Confirms every write is acknowledged by the OS and that the last item
    /// replayed is the one left on the clipboard — the ordering guarantee a
    /// reseed depends on.
    #[test]
    #[ignore]
    fn every_replayed_item_is_acknowledged_and_the_last_one_wins() {
        let _serial = exclusive();
        let dir = std::env::temp_dir();
        let mut acknowledged = 0;
        for n in 1..=5 {
            let item = ClipboardItem {
                id: format!("probe_{n}"),
                kind: ClipboardKind::Text,
                order: n,
                text: Some(format!("PC-SNAPSHOT-SEQ-{n}")),
                sidecar: None,
                source: "current".into(),
                byte_size: 0,
            };
            if replay_one(&dir, &item).expect("replay failed") {
                acknowledged += 1;
            }
        }
        assert_eq!(acknowledged, 5, "some writes were never acknowledged by Windows");

        let (read, warnings) = win32::capture_current(&dir, "probe_seq");
        let read = read.unwrap_or_else(|| panic!("clipboard empty after replay: {warnings:?}"));
        assert_eq!(read.text.as_deref(), Some("PC-SNAPSHOT-SEQ-5"));
    }

    /// The whole point of the fallback: a snapshot records what the user has
    /// copied even when the history service is unreachable. Holds either way —
    /// if history *is* healthy it returns the same item as its newest entry.
    #[test]
    #[ignore]
    fn capture_still_yields_the_current_item_when_history_is_unavailable() {
        let _serial = exclusive();
        let dir = std::env::temp_dir();
        let item = ClipboardItem {
            id: "probe".into(),
            kind: ClipboardKind::Text,
            order: 0,
            text: Some("PC-SNAPSHOT-FALLBACK".into()),
            sidecar: None,
            source: "current".into(),
            byte_size: 0,
        };
        win32::copy_item(&dir, &item).expect("seed the clipboard");

        let (block, warnings) = capture(&dir, "probe_fb");
        println!("capture warnings: {warnings:?}");
        let block = block.expect("capture produced no clipboard block");
        assert!(
            block
                .items
                .iter()
                .any(|i| i.text.as_deref() == Some("PC-SNAPSHOT-FALLBACK")),
            "captured items did not include the seeded clipboard text"
        );
    }

    #[test]
    fn bounded_gives_up_instead_of_blocking_its_caller_forever() {
        let err = bounded("Clipboard capture", Duration::from_millis(120), || {
            std::thread::sleep(Duration::from_secs(30));
        })
        .unwrap_err();
        assert!(err.contains("timed out"), "unexpected message: {err}");
    }
}

/// Place one item on the clipboard and wait for the OS to acknowledge it before
/// returning, instead of sleeping a guessed interval and hoping.
///
/// A fixed delay let replayed writes overwrite each other before the history
/// service had consumed them — which is why a restore came back with only some
/// of its items, in no particular order. Returns whether the write was
/// acknowledged (the clipboard sequence number moved) within the timeout.
#[cfg(windows)]
fn replay_one(dir: &Path, item: &ClipboardItem) -> Result<bool, String> {
    let before = win32::sequence_number();
    win32::copy_item(dir, item)?;

    let deadline = std::time::Instant::now() + REPLAY_CONFIRM_TIMEOUT;
    while win32::sequence_number() == before && std::time::Instant::now() < deadline {
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    let acknowledged = win32::sequence_number() != before;

    // The sequence number moves the moment the clipboard changes, but the
    // history service consumes that change asynchronously — this is the gap it
    // needs to file the item as its own tile before the next one lands on top.
    // It scales with payload size: text is filed almost instantly, while a
    // several-hundred-KB image becomes a multi-megabyte DIB the service has to
    // copy into its own store, and a flat delay dropped the big ones.
    std::thread::sleep(settle_for(item));
    Ok(acknowledged)
}

/// Settle time for one replayed item, scaled by payload size and capped.
#[cfg(windows)]
fn settle_for(item: &ClipboardItem) -> std::time::Duration {
    let extra = (item.byte_size / (128 * 1024)) * 150;
    REPLAY_SETTLE + std::time::Duration::from_millis(extra.min(1_500))
}

#[cfg(windows)]
pub fn reseed_history(dir: &Path, block: &ClipboardBlock, backup_ok: bool) -> Vec<String> {
    let mut warnings = Vec::new();

    // SAFETY INVARIANT: never clear without a verified backup.
    if !backup_ok {
        warnings.push(
            "Clipboard not reseeded: pre-restore backup could not be verified; live Win+V left untouched".into(),
        );
        return warnings;
    }

    // Clearing is the only destructive step and the only one that needs the
    // history service, so it is time-boxed — and a failure is *not* fatal:
    // replaying on top of the existing history still hands the items back, it
    // just leaves the old ones underneath.
    match bounded("Clipboard history clear", CLEAR_TIMEOUT, win::clear_history) {
        Ok(Ok(())) => {}
        Ok(Err(e)) => warnings.push(format!(
            "Win+V history not cleared ({e}); restored items were added on top"
        )),
        Err(e) => warnings.push(format!("{e}; restored items were added on top")),
    }

    // The replay goes through Win32, not WinRT. `Clipboard::SetContent` rides
    // the history service, so while that service was unwell every single item
    // failed to reseed — the restore cleared the user's clipboard and put
    // nothing back. Win32 writes land regardless, and the history service picks
    // them up as clipboard changes whenever it is healthy.
    let mut ordered: Vec<&ClipboardItem> = block.items.iter().collect();
    ordered.sort_by_key(|item| item.order); // oldest first, so the newest ends up on top
    let mut restored = 0usize;
    let mut confirmed = 0usize;
    for item in &ordered {
        match replay_one(dir, item) {
            Ok(acknowledged) => {
                restored += 1;
                if acknowledged {
                    confirmed += 1;
                }
            }
            Err(e) => warnings.push(format!("Clipboard item not reseeded: {e}")),
        }
    }

    if restored == 0 && !ordered.is_empty() {
        warnings.push("No clipboard items could be restored".into());
    } else if confirmed < ordered.len() {
        warnings.push(format!(
            "Only {confirmed} of {} items were registered by Windows; the clipboard history service dropped the rest",
            ordered.len()
        ));
    }
    warnings
}
