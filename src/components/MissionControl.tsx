import { useEffect, useMemo, useRef, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { getSnapshot, getAppIcon } from "../commands/snapshots";
import { copyClipboardItem, restoreClipboard } from "../commands/clipboard";
import type { ActivityEvent, ProcessInfo, Snapshot, SnapshotSummary } from "../types/snapshot";
import { thumbnailUrl } from "../utils/thumbnail";
import { ActivityLogModal } from "./ActivityLogModal";
import { SettingsMenu } from "./SettingsMenu";
import { SettingsPage } from "./SettingsPage";

type Props = {
  snapshots: SnapshotSummary[]; events: ActivityEvent[]; selectedId: string | null; activeSessionId: string | null;
  onSelect: (id: string | null) => void; onCapture: () => void; onStartNew: () => void;
  onRestore: (id: string) => void; onDelete: (id: string) => void; onRecapture: (id: string) => void;
  onDuplicate: (id: string) => void; onRename: (id: string) => void;
  onRestoreApp: (id: string, exePath: string, appName: string) => void;
  onRestoreExplorer: (id: string) => void; restoringAppKey: string | null;
  onClearAll: () => void; onImport: () => void; onHelp: () => void; onRefresh: () => void;
  onIgnoreList: () => void; onToggleTerminalHook: () => void; terminalHookEnabled: boolean;
};

const relative = (stamp: string) => {
  const d = Date.now() - new Date(stamp).getTime();
  if (d < 60000) return "Just now"; if (d < 3600000) return `${Math.floor(d / 60000)}m ago`;
  if (d < 86400000) return `Today ${new Date(stamp).toLocaleTimeString([], { hour: "numeric", minute: "2-digit" })}`;
  return new Date(stamp).toLocaleDateString([], { weekday: "short", month: "short", day: "numeric" });
};

// Absolute capture time printed on the tile, e.g. "Jul 21, 2026, 3:26 PM".
const absTime = (stamp: string) =>
  new Date(stamp).toLocaleString([], { month: "short", day: "numeric", year: "numeric", hour: "numeric", minute: "2-digit" });

function Icon({ children }: { children: React.ReactNode }) { return <span className="rail-icon">{children}</span>; }

// Screenshots vary wildly in mean luminance — a dark IDE vs. a white Explorer
// window — so the single uniform dim applied in CSS makes bright captures glare
// against the dark grid. We sample each thumbnail's average luminance once and
// pull its resting brightness toward a shared target, dimming bright shots and
// gently lifting dark ones so the grid reads as one surface. Measurement runs on
// a detached crossOrigin image: if the webview taints the canvas the probe just
// errors and we keep the CSS default, never touching the visible thumbnail.
// Result is cached per URL (the URL carries the capture revision, so a recapture
// re-measures automatically).
const thumbNormCache = new Map<string, number>();

function normFromLuma(luma: number): number {
  const full = 0.42 / Math.max(luma, 0.05); // brightness that lands the mean on target
  const b = 0.72 * 0.2 + full * 0.8;         // blend 80% normalized, 20% uniform dim
  return Math.min(1.8, Math.max(0.55, b));
}

function measureThumbNorm(src: string): Promise<number | null> {
  return new Promise(resolve => {
    const probe = new Image();
    probe.crossOrigin = "anonymous";
    probe.onload = () => {
      try {
        const c = document.createElement("canvas");
        c.width = 24; c.height = 15;
        const ctx = c.getContext("2d", { willReadFrequently: true });
        if (!ctx) return resolve(null);
        ctx.drawImage(probe, 0, 0, c.width, c.height);
        const { data } = ctx.getImageData(0, 0, c.width, c.height);
        let sum = 0;
        for (let i = 0; i < data.length; i += 4)
          sum += 0.2126 * data[i] + 0.7152 * data[i + 1] + 0.0722 * data[i + 2];
        resolve(normFromLuma(sum / (data.length / 4) / 255));
      } catch { resolve(null); } // tainted canvas — degrade to the CSS default
    };
    probe.onerror = () => resolve(null);
    probe.src = src;
  });
}

// Thumbnail that self-normalizes its resting brightness via a CSS var (--norm);
// falls back to the stylesheet default until/unless a measurement lands.
function NormalizedThumb({ src }: { src: string }) {
  const ref = useRef<HTMLImageElement>(null);
  useEffect(() => {
    let alive = true;
    const set = (n: number) => { if (alive) ref.current?.style.setProperty("--norm", n.toFixed(3)); };
    const cached = thumbNormCache.get(src);
    if (cached != null) { set(cached); return; }
    measureThumbNorm(src).then(n => {
      if (n == null) return;
      thumbNormCache.set(src, n);
      set(n);
    });
    return () => { alive = false; };
  }, [src]);
  return <img ref={ref} src={src} alt="" />;
}

// Real exe icon for a captured app, resolved lazily and cached per path across
// snapshots. Falls back to the two-letter monogram when the icon can't be read
// (empty path, UWP stub, non-Windows) so a row always renders something.
const iconCache = new Map<string, string | null>();
function AppIcon({ proc }: { proc: ProcessInfo }) {
  const [uri, setUri] = useState<string | null | undefined>(() => iconCache.get(proc.exe_path));
  useEffect(() => {
    if (iconCache.has(proc.exe_path)) { setUri(iconCache.get(proc.exe_path)); return; }
    if (!proc.exe_path) { iconCache.set("", null); setUri(null); return; }
    let alive = true;
    getAppIcon(proc.exe_path)
      .then(u => { iconCache.set(proc.exe_path, u); if (alive) setUri(u); })
      .catch(() => { if (alive) setUri(null); });
    return () => { alive = false; };
  }, [proc.exe_path]);
  if (uri) return <img className="app-icon" src={uri} alt="" />;
  return <span className="monogram">{proc.name.slice(0, 2)}</span>;
}

// Compact icon for the tile app-stack, resolved by exe path via the shared cache;
// falls back to a two-letter monogram derived from the exe stem.
function MiniAppIcon({ path }: { path: string }) {
  const [uri, setUri] = useState<string | null | undefined>(() => iconCache.get(path));
  useEffect(() => {
    if (iconCache.has(path)) { setUri(iconCache.get(path)); return; }
    let alive = true;
    getAppIcon(path)
      .then(u => { iconCache.set(path, u); if (alive) setUri(u); })
      .catch(() => { if (alive) setUri(null); });
    return () => { alive = false; };
  }, [path]);
  if (uri) return <img className="mini-icon" src={uri} alt="" />;
  const stem = (path.split(/[\\/]/).pop() ?? "").replace(/\.exe$/i, "");
  return <span className="mini-icon mono">{stem.slice(0, 2)}</span>;
}

export function MissionControl(p: Props) {
  const [search, setSearch] = useState("");
  const [details, setDetails] = useState<Snapshot | null>(null);
  const [showPicker, setShowPicker] = useState(false);
  const [showSettings, setShowSettings] = useState(false);
  const [menuOpenId, setMenuOpenId] = useState<string | null>(null);
  const [logEvent, setLogEvent] = useState<ActivityEvent | null>(null);
  const [clipOpen, setClipOpen] = useState(false);
  const [clipCopied, setClipCopied] = useState<{ id: string; ok: boolean } | null>(null);
  const [clipRestore, setClipRestore] = useState<"idle" | "busy" | "done" | "failed">("idle");
  const [rightWidth, setRightWidth] = useState(290);
  const dragRef = useRef<{ x: number; w: number } | null>(null);
  const onDragMove = (e: MouseEvent) => { if (!dragRef.current) return; setRightWidth(Math.min(480, Math.max(220, dragRef.current.w + (dragRef.current.x - e.clientX)))); };
  const onDragEnd = () => { dragRef.current = null; window.removeEventListener("mousemove", onDragMove); window.removeEventListener("mouseup", onDragEnd); };
  const onDragStart = (e: React.MouseEvent) => { e.preventDefault(); dragRef.current = { x: e.clientX, w: rightWidth }; window.addEventListener("mousemove", onDragMove); window.addEventListener("mouseup", onDragEnd); };
  const searchRef = useRef<HTMLInputElement>(null);
  const clipCopyTimer = useRef<number | null>(null);
  const clipRestoreTimer = useRef<number | null>(null);
  const selected = p.snapshots.find(s => s.id === p.selectedId);
  const filtered = useMemo(() => p.snapshots.filter(s => s.name.toLowerCase().includes(search.toLowerCase())), [p.snapshots, search]);
  useEffect(() => {
    if (clipCopyTimer.current !== null) { window.clearTimeout(clipCopyTimer.current); clipCopyTimer.current = null; }
    if (clipRestoreTimer.current !== null) { window.clearTimeout(clipRestoreTimer.current); clipRestoreTimer.current = null; }
    setClipOpen(false); setClipCopied(null); setClipRestore("idle");
    if (!p.selectedId) return;
    let cancelled = false;
    getSnapshot(p.selectedId)
      .then(snapshot => { if (!cancelled) setDetails(snapshot); })
      .catch(() => { if (!cancelled) setDetails(null); });
    return () => { cancelled = true; };
  }, [p.selectedId, selected?.timestamp]);
  // Any click outside an open card menu closes it. The kebab toggle stops
  // propagation, so the opening click never reaches this listener.
  useEffect(() => {
    if (!menuOpenId) return;
    const close = () => setMenuOpenId(null);
    window.addEventListener("click", close);
    return () => window.removeEventListener("click", close);
  }, [menuOpenId]);
  useEffect(() => {
    const key = (e: KeyboardEvent) => {
      if (e.ctrlKey && e.key.toLowerCase() === "s") { e.preventDefault(); p.onCapture(); }
      if (e.ctrlKey && e.key.toLowerCase() === "k") { e.preventDefault(); searchRef.current?.focus(); }
      // Settings is an overlay now and closes itself on Escape (with its exit animation).
      if (e.key === "Escape" && !showSettings) p.onSelect(null);
      const interactive = e.target instanceof HTMLElement && e.target.closest("button,input,textarea,select,[contenteditable='true']");
      if (!interactive && !showSettings && p.selectedId && e.key === "Enter") p.onRestore(p.selectedId);
      if (!interactive && !showSettings && p.selectedId && e.key === "Delete") p.onDelete(p.selectedId);
    }; window.addEventListener("keydown", key); return () => window.removeEventListener("keydown", key);
  }, [p, showSettings]);
  return <div className="app-frame" style={{ "--right-w": `${rightWidth}px` } as React.CSSProperties}>
    <div
      className="titlebar-reveal-zone"
      onMouseDown={e => {
        if (e.button === 0 && !(e.target as Element).closest("button")) {
          getCurrentWindow().startDragging().catch(() => {});
        }
      }}
    >
      <header className="titlebar">
        <div className="window-actions">
          <button type="button" aria-label="Minimize" onClick={() => getCurrentWindow().minimize()}><svg width="11" height="11" viewBox="0 0 10 10" shapeRendering="crispEdges"><line x1="1" y1="5" x2="9" y2="5" stroke="currentColor" strokeWidth="1"/></svg></button><button type="button" aria-label="Maximize" onClick={() => getCurrentWindow().toggleMaximize()}><svg width="11" height="11" viewBox="0 0 10 10" shapeRendering="crispEdges"><rect x="1" y="1" width="8" height="8" fill="none" stroke="currentColor" strokeWidth="1"/></svg></button><button type="button" aria-label="Close" className="close" onClick={() => getCurrentWindow().close()}><svg width="11" height="11" viewBox="0 0 10 10"><line x1="1.5" y1="1.5" x2="8.5" y2="8.5" stroke="currentColor" strokeWidth="1.2" strokeLinecap="round"/><line x1="8.5" y1="1.5" x2="1.5" y2="8.5" stroke="currentColor" strokeWidth="1.2" strokeLinecap="round"/></svg></button>
        </div>
      </header>
    </div>
    <>
    <aside className="sidebar">
      <button className="rail-button active capture-rail-button" onClick={p.onCapture}><Icon>◉</Icon><span>Capture</span></button>
      <button className="rail-button start-new-rail-button" onClick={p.onStartNew}><Icon>＋</Icon><span>Start new</span></button>
      <button className="rail-button restore-rail-button" onClick={() => p.selectedId ? p.onRestore(p.selectedId) : setShowPicker(true)}><Icon>↻</Icon><span>Restore</span></button>
      <div className="rail-spacer"/>
      <SettingsMenu open={showSettings} onToggle={() => setShowSettings(v => !v)}/>
    </aside>
    <main className="center-panel">
      {p.snapshots.length === 0 ? <div className="mission-empty"><div className="empty-mark">□</div><h1>Save your first workspace</h1><p>PC Snapshot remembers your open apps, windows, tabs and terminal — so you can bring the whole setup back in one click. Everything stays on this PC.</p><button className="primary" onClick={p.onCapture}>◉ Capture my desktop <kbd>Ctrl S</kbd></button><button className="link" onClick={p.onImport}>or import snapshots from a backup</button></div> : <>
        <div className="grid-header"><h1>All snapshots <small>{p.snapshots.length}</small></h1><div className="search">⌕ <input ref={searchRef} value={search} onChange={e => setSearch(e.target.value)} placeholder="Search or filter"/></div></div>
        <div className="snapshot-grid">{filtered.map(s => {
          const working = p.activeSessionId === s.id;
          const status = working ? "#48b4ff" : s.warning_count ? "#f0b429" : "#46c98b";
          const ink = working ? "#04121f" : s.warning_count ? "#241a00" : "#06210f";
          const chip = working ? "Currently working" : s.warning_count ? `${s.warning_count} warning${s.warning_count === 1 ? "" : "s"}` : relative(s.timestamp);
          const apps = s.top_apps ?? [];
          return <article key={s.id}
            className={`snapshot-card ${p.selectedId === s.id ? "selected" : ""} ${menuOpenId === s.id ? "menu-open" : ""} ${working ? "is-working" : ""}`}
            style={{ "--status": status, "--status-ink": ink } as React.CSSProperties}
            onClick={() => p.onSelect(p.selectedId === s.id ? null : s.id)}>
            <div className="card-thumb">
              {s.thumbnail_path ? <NormalizedThumb src={thumbnailUrl(s.thumbnail_path, s.timestamp)}/> : <div className="thumb-placeholder"/>}
              <div className="card-scrim"/>
              <span className="status-chip"><i/>{chip}</span>
              <div className="card-hover"><button className="card-restore" onClick={e => {e.stopPropagation(); p.onRestore(s.id)}}>Restore</button></div>
            </div>
            <div className="card-overlay">
              <strong className="card-title">{s.name}</strong>
              <div className="card-apps">
                {apps.length > 0 && <span className="mini-stack">{apps.slice(0, 4).map(path => <MiniAppIcon key={path} path={path}/>)}</span>}
                <span className="card-meta">{s.app_count} app{s.app_count === 1 ? "" : "s"} · {s.monitor_count} monitor{s.monitor_count === 1 ? "" : "s"}</span>
              </div>
              <span className="card-time">{absTime(s.timestamp)}</span>
              <button className="card-rename" aria-label={`Rename ${s.name}`} title="Rename" onClick={e => {e.stopPropagation(); p.onRename(s.id)}}><svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true"><path d="M12 20h9"/><path d="M16.5 3.5a2.1 2.1 0 0 1 3 3L8 18l-4 1 1-4Z"/></svg></button>
            </div>
            <button className="thumb-menu-btn" aria-label={`More actions for ${s.name}`} aria-haspopup="menu" aria-expanded={menuOpenId === s.id} title="More" onClick={e => {e.stopPropagation(); setMenuOpenId(id => id === s.id ? null : s.id)}}><svg width="16" height="16" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true"><circle cx="5" cy="12" r="1.6"/><circle cx="12" cy="12" r="1.6"/><circle cx="19" cy="12" r="1.6"/></svg></button>
            {menuOpenId === s.id && <div className="card-menu" role="menu" onClick={e => e.stopPropagation()}>
              <button role="menuitem" onClick={e => {e.stopPropagation(); setMenuOpenId(null); p.onRecapture(s.id)}}><svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true"><polyline points="23 4 23 10 17 10"/><path d="M20.49 15a9 9 0 1 1-2.12-9.36L23 10"/></svg>Recapture</button>
              <button role="menuitem" onClick={e => {e.stopPropagation(); setMenuOpenId(null); p.onDuplicate(s.id)}}><svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true"><rect x="9" y="9" width="12" height="12" rx="2"/><path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1"/></svg>Duplicate</button>
              <div className="sep"/>
              <button role="menuitem" className="danger" onClick={e => {e.stopPropagation(); setMenuOpenId(null); p.onDelete(s.id)}}><svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true"><polyline points="3 6 5 6 21 6"/><path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6m3 0V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2"/></svg>Delete</button>
            </div>}
          </article>;
        })}</div>
      </>}
    </main>
    <aside className={`right-panel ${p.selectedId ? "show-details" : ""}`}>
      <div className="resizer" onMouseDown={onDragStart}/>
      <section className="panel-page activity"><div className="panel-title"><span><b className="good">●</b> Activity</span></div><div className="event-list">{p.events.length === 0 ? <p className="muted">Actions you take will appear here.</p> : p.events.map(e => <div className="event" key={e.id}><div className="event-meta">— {e.kind.replace("_", " ")} · {relative(e.timestamp)} —</div><strong className={e.status}>{e.status === "success" ? "✓" : "!"} {e.summary}</strong>{e.detail_lines.map((d,i) => <p key={i}>› {d}</p>)}{e.status !== "success" && <button className="event-logs" onClick={() => setLogEvent(e)}>Show logs</button>}</div>)}</div></section>
      <section className="panel-page details">
        <div className="detail-scroll">
          <button className="back" onClick={() => p.onSelect(null)}>← Activity</button>
          <div className="detail-preview">{selected?.thumbnail_path && <img src={thumbnailUrl(selected.thumbnail_path, selected.timestamp)} alt=""/>}<span>preview · {new Set([...(details?.windows ?? []), ...(details?.explorer_windows ?? [])].map(w => w.monitor_index)).size || 1} monitors</span></div>
          <h2>{selected?.name}</h2>
          <p className="muted">Captured {selected ? relative(selected.timestamp).toLowerCase() : ""}</p>
          <p className={selected?.warning_count ? "warning-text" : "success-text"}>● {selected?.warning_count ? `Captured with ${selected.warning_count} warning${selected.warning_count === 1 ? "" : "s"}` : "Captured successfully"}</p>
          {details && details.warnings.length > 0 && <div className="snapshot-warnings" role="status" aria-label="Capture warnings">
            <div className="warning-heading">Warning details</div>
            {details.warnings.map((warning, index) => <div className="warning-message" key={`${index}-${warning}`}><span>!</span><p>{warning}</p></div>)}
          </div>}
          <div className="contents-head"><span>CONTENTS</span><span>{details ? details.processes.length + (details.explorer_windows?.length ? 1 : 0) : "…"} apps</span></div>
          {!!details?.explorer_windows?.length && (() => {
            const restoreKey = `${details.id}:explorer`;
            const restoring = p.restoringAppKey === restoreKey;
            return <div className="app-row explorer-row">
              <AppIcon proc={{ name: "File Explorer", pid: 0, exe_path: "C:\\Windows\\explorer.exe", cmd_line: "", classification: "background" }}/>
              <b title={details.explorer_windows.map(window => window.path).join("\n")}>File Explorer</b>
              <span className="app-row-trailing"><small className="app-count">{details.explorer_windows.length}</small><button
                type="button"
                className={`app-restore-button ${restoring ? "restoring" : ""}`}
                disabled={restoring || p.restoringAppKey !== null}
                aria-label="Restore File Explorer"
                title="Restore captured File Explorer folders"
                onClick={e => { e.stopPropagation(); if (p.selectedId) p.onRestoreExplorer(p.selectedId); }}
              ><svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true"><path d="M3 12a9 9 0 1 0 3-6.7"/><path d="M3 4v6h6"/></svg></button></span>
            </div>;
          })()}
          {details?.processes.map(proc => {
            const appName = proc.name.replace(/\.exe$/i, "");
            const restoreKey = `${details.id}:${proc.exe_path.toLowerCase()}`;
            const restoring = p.restoringAppKey === restoreKey;
            const windowCount = details.windows.filter(w => w.exe_path?.toLowerCase() === proc.exe_path.toLowerCase()).length;
            return <div className="app-row" key={`${proc.pid}-${proc.name}`}><AppIcon proc={proc}/><b>{appName}</b><span className="app-row-trailing"><small className="app-count">{windowCount || ""}</small><button
              type="button"
              className={`app-restore-button ${restoring ? "restoring" : ""}`}
              disabled={!proc.exe_path || restoring || p.restoringAppKey !== null}
              aria-label={`Restore ${appName}`}
              title={proc.exe_path ? `Restore ${appName}` : "Restore unavailable: executable path was not captured"}
              onClick={e => { e.stopPropagation(); if (p.selectedId && proc.exe_path) p.onRestoreApp(p.selectedId, proc.exe_path, appName); }}
            ><svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true"><path d="M3 12a9 9 0 1 0 3-6.7"/><path d="M3 4v6h6"/></svg></button></span></div>;
          })}
          {!!details?.clipboard?.items?.length && (() => {
            const clip = details.clipboard;
            const items = [...clip.items].sort((a, b) => b.order - a.order);
            const dir = details.thumbnail_path.replace(/[^\\/]+$/, "");
            // Every press re-issues the copy, even while the tick is still
            // showing — the timer is re-armed rather than left to expire early.
            // A rejected copy shows a cross rather than nothing: silence here
            // is what made a wedged clipboard service look like a dead button.
            const copy = (itemId: string) => {
              const settle = (ok: boolean) => {
                if (clipCopyTimer.current !== null) window.clearTimeout(clipCopyTimer.current);
                setClipCopied({ id: itemId, ok });
                clipCopyTimer.current = window.setTimeout(() => { clipCopyTimer.current = null; setClipCopied(null); }, ok ? 1400 : 2600);
              };
              copyClipboardItem("snapshot", details.id, itemId)
                .then(() => settle(true))
                .catch(() => settle(false));
            };
            const restoreAll = () => {
              if (clipRestore === "busy") return;
              if (clipRestoreTimer.current !== null) window.clearTimeout(clipRestoreTimer.current);
              setClipRestore("busy");
              restoreClipboard(details.id)
                .then(warnings => setClipRestore(warnings.length ? "failed" : "done"))
                .catch(() => setClipRestore("failed"))
                .finally(() => {
                  clipRestoreTimer.current = window.setTimeout(() => { clipRestoreTimer.current = null; setClipRestore("idle"); }, 2200);
                });
            };
            return <div className={`clip-section ${clipOpen ? "open" : ""}`}>
              <div className="clip-head">
                <button type="button" className="clip-toggle" aria-expanded={clipOpen} onClick={() => setClipOpen(v => !v)}>
                  <svg className="clip-chevron" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.6" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true"><polyline points="9 18 15 12 9 6"/></svg>
                  <span>CLIPBOARD</span>
                  <small>{items.length} item{items.length === 1 ? "" : "s"}</small>
                </button>
                <button type="button" className={`clip-restore-btn ${clipRestore}`} disabled={clipRestore === "busy"}
                  aria-label="Restore clipboard history"
                  title={clipRestore === "failed" ? "Clipboard restored with warnings — see Activity" : "Restore this snapshot's clipboard into Win+V (your current clipboard is backed up first)"}
                  onClick={e => { e.stopPropagation(); restoreAll(); }}>
                  {clipRestore === "done"
                    ? <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.4" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true"><polyline points="20 6 9 17 4 12"/></svg>
                    : <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true"><path d="M3 12a9 9 0 1 0 3-6.7"/><path d="M3 4v6h6"/></svg>}
                </button>
              </div>
              {clipOpen && <div className="clip-list">{items.map(item => {
                const isImage = item.kind === "image" && !!item.sidecar;
                const text = (item.text ?? "").trim();
                return <div className="clip-row" key={item.id}>
                  {isImage
                    ? <img className="clip-thumb" src={thumbnailUrl(dir + item.sidecar, details.timestamp)} alt="Clipboard image"/>
                    : <span className="clip-glyph" aria-hidden="true">T</span>}
                  <div className="clip-copy">
                    <span className="clip-text">{isImage ? "Image" : (text.slice(0, 160) || "(empty)")}</span>
                    {isImage && <span className="clip-sub">{Math.max(1, Math.round(item.byte_size / 1024))} KB</span>}
                  </div>
                  {(() => {
                    const state = clipCopied?.id === item.id ? (clipCopied.ok ? "copied" : "copy-failed") : "";
                    return <button type="button" className={`clip-copy-btn ${state}`}
                      aria-label={isImage ? "Copy image to clipboard" : "Copy text to clipboard"}
                      title={state === "copy-failed" ? "Copy failed — Windows did not accept the clipboard write" : "Copy to clipboard"}
                      onClick={e => { e.stopPropagation(); copy(item.id); }}>
                      {state === "copied"
                        ? <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.4" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true"><polyline points="20 6 9 17 4 12"/></svg>
                        : state === "copy-failed"
                          ? <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.4" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true"><line x1="18" y1="6" x2="6" y2="18"/><line x1="6" y1="6" x2="18" y2="18"/></svg>
                          : <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true"><rect x="9" y="9" width="12" height="12" rx="2"/><path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1"/></svg>}
                    </button>;
                  })()}
                </div>;
              })}</div>}
            </div>;
          })()}
        </div>
        <div className="detail-actions"><button className="primary restore-action" onClick={() => p.selectedId && p.onRestore(p.selectedId)}>↻ Restore</button><button className="recapture-action" aria-label="Recapture" title="Recapture" onClick={() => p.selectedId && p.onRecapture(p.selectedId)}>↻</button><button className="danger" aria-label="Delete" onClick={() => p.selectedId && p.onDelete(p.selectedId)}><svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"><polyline points="3 6 5 6 21 6"/><path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6m3 0V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2"/><line x1="10" y1="11" x2="10" y2="17"/><line x1="14" y1="11" x2="14" y2="17"/></svg></button></div>
      </section>
    </aside>
    </>
    <ActivityLogModal event={logEvent} onDismiss={() => setLogEvent(null)}/>
    {showSettings && <SettingsPage snapshots={p.snapshots} terminalHookEnabled={p.terminalHookEnabled}
      onToggleTerminalHook={p.onToggleTerminalHook} onClearAll={p.onClearAll} onImport={p.onImport}
      onHelp={p.onHelp} onRefresh={p.onRefresh} onClose={() => setShowSettings(false)}/>} 
    {showPicker && <div className="modal-backdrop" onMouseDown={e => e.target === e.currentTarget && setShowPicker(false)}>
      <div className="picker-modal">
        <h2>Select a snapshot to restore</h2>
        <p>Choose which saved snapshot to bring back.</p>
        <div className="picker-list">
          {p.snapshots.length === 0 ? <div className="picker-empty">No snapshots saved yet.</div> : p.snapshots.map(s =>
            <button key={s.id} className="picker-row" onClick={() => { setShowPicker(false); p.onRestore(s.id); }}>
              <span className="thumb-sm">{s.thumbnail_path && <img src={thumbnailUrl(s.thumbnail_path, s.timestamp)} alt=""/>}</span>
              <span className="picker-row-copy"><b>{s.name}</b><small>{relative(s.timestamp)}</small></span>
            </button>
          )}
        </div>
        <div className="modal-actions"><button onClick={() => setShowPicker(false)}>Cancel</button></div>
      </div>
    </div>}
  </div>;
}

export function StartNewModal({ open, busy, onCancel, onConfirm }: { open: boolean; busy: boolean; onCancel: () => void; onConfirm: (saveFirst: boolean) => void }) {
  const [saveFirst, setSaveFirst] = useState(true); if (!open) return null;
  return <div className="modal-backdrop" onMouseDown={e => e.target === e.currentTarget && onCancel()}><div className="start-modal"><h2>Start a new session?</h2><p>This gracefully closes open app windows so you can return to a clean desktop.</p><button className={`save-toggle ${saveFirst ? "on" : ""}`} onClick={() => setSaveFirst(v => !v)}><span>{saveFirst ? "◉" : "◯"}</span><div><b>Save current desktop first</b><small>Recommended — you can come back to exactly this.</small></div><i>{saveFirst ? "ON" : "OFF"}</i></button><div className="modal-actions"><button onClick={onCancel}>Cancel</button><button className="destructive" disabled={busy} onClick={() => onConfirm(saveFirst)}>{busy ? "Starting…" : saveFirst ? "Save & start fresh" : "Start fresh"}</button></div></div></div>;
}
