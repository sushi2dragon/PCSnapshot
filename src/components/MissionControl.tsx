import { useEffect, useMemo, useRef, useState } from "react";
import { convertFileSrc } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { getSnapshot, getAppIcon } from "../commands/snapshots";
import type { ActivityEvent, ProcessInfo, Snapshot, SnapshotSummary } from "../types/snapshot";
import { SettingsMenu } from "./SettingsMenu";
import { SettingsPage } from "./SettingsPage";

type Props = {
  snapshots: SnapshotSummary[]; events: ActivityEvent[]; selectedId: string | null; activeSessionId: string | null;
  onSelect: (id: string | null) => void; onCapture: () => void; onStartNew: () => void;
  onRestore: (id: string) => void; onDelete: (id: string) => void; onRecapture: (id: string) => void;
  onClearAll: () => void; onImport: () => void; onHelp: () => void; onRefresh: () => void;
  onIgnoreList: () => void; onToggleTerminalHook: () => void; terminalHookEnabled: boolean;
};

const relative = (stamp: string) => {
  const d = Date.now() - new Date(stamp).getTime();
  if (d < 60000) return "Just now"; if (d < 3600000) return `${Math.floor(d / 60000)}m ago`;
  if (d < 86400000) return `Today ${new Date(stamp).toLocaleTimeString([], { hour: "numeric", minute: "2-digit" })}`;
  return new Date(stamp).toLocaleDateString([], { weekday: "short", month: "short", day: "numeric" });
};

function Icon({ children }: { children: React.ReactNode }) { return <span className="rail-icon">{children}</span>; }

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

export function MissionControl(p: Props) {
  const [search, setSearch] = useState("");
  const [details, setDetails] = useState<Snapshot | null>(null);
  const [showPicker, setShowPicker] = useState(false);
  const [showSettings, setShowSettings] = useState(false);
  const [rightWidth, setRightWidth] = useState(290);
  const dragRef = useRef<{ x: number; w: number } | null>(null);
  const onDragMove = (e: MouseEvent) => { if (!dragRef.current) return; setRightWidth(Math.min(480, Math.max(220, dragRef.current.w + (dragRef.current.x - e.clientX)))); };
  const onDragEnd = () => { dragRef.current = null; window.removeEventListener("mousemove", onDragMove); window.removeEventListener("mouseup", onDragEnd); };
  const onDragStart = (e: React.MouseEvent) => { e.preventDefault(); dragRef.current = { x: e.clientX, w: rightWidth }; window.addEventListener("mousemove", onDragMove); window.addEventListener("mouseup", onDragEnd); };
  const searchRef = useRef<HTMLInputElement>(null);
  const selected = p.snapshots.find(s => s.id === p.selectedId);
  const filtered = useMemo(() => p.snapshots.filter(s => s.name.toLowerCase().includes(search.toLowerCase())), [p.snapshots, search]);
  useEffect(() => {
    if (!p.selectedId) return;
    getSnapshot(p.selectedId).then(setDetails).catch(() => setDetails(null));
  }, [p.selectedId]);
  useEffect(() => {
    const key = (e: KeyboardEvent) => {
      if (e.ctrlKey && e.key.toLowerCase() === "s") { e.preventDefault(); p.onCapture(); }
      if (e.ctrlKey && e.key.toLowerCase() === "k") { e.preventDefault(); searchRef.current?.focus(); }
      if (e.key === "Escape") { if (showSettings) setShowSettings(false); else p.onSelect(null); }
      if (p.selectedId && e.key === "Enter") p.onRestore(p.selectedId);
      if (p.selectedId && e.key === "Delete") p.onDelete(p.selectedId);
    }; window.addEventListener("keydown", key); return () => window.removeEventListener("keydown", key);
  }, [p, showSettings]);
  return <div className="app-frame" style={{ "--right-w": `${rightWidth}px` } as React.CSSProperties}>
    <header className="titlebar" data-tauri-drag-region><span className="brand-mark" data-tauri-drag-region/> <span data-tauri-drag-region>PC Snapshot</span><div className="window-actions">
      <button type="button" aria-label="Minimize" onClick={() => getCurrentWindow().minimize()}><svg width="11" height="11" viewBox="0 0 10 10" shapeRendering="crispEdges"><line x1="1" y1="5" x2="9" y2="5" stroke="currentColor" strokeWidth="1"/></svg></button><button type="button" aria-label="Maximize" onClick={() => getCurrentWindow().toggleMaximize()}><svg width="11" height="11" viewBox="0 0 10 10" shapeRendering="crispEdges"><rect x="1" y="1" width="8" height="8" fill="none" stroke="currentColor" strokeWidth="1"/></svg></button><button type="button" aria-label="Close" className="close" onClick={() => getCurrentWindow().close()}><svg width="11" height="11" viewBox="0 0 10 10"><line x1="1.5" y1="1.5" x2="8.5" y2="8.5" stroke="currentColor" strokeWidth="1.2" strokeLinecap="round"/><line x1="8.5" y1="1.5" x2="1.5" y2="8.5" stroke="currentColor" strokeWidth="1.2" strokeLinecap="round"/></svg></button>
    </div></header>
    {!showSettings && <>
    <aside className="sidebar">
      <button className="rail-button active" onClick={p.onCapture}><Icon>◉</Icon><span>Capture</span></button>
      <button className="rail-button" onClick={p.onStartNew}><Icon>＋</Icon><span>Start new</span></button>
      <button className="rail-button" onClick={() => p.selectedId ? p.onRestore(p.selectedId) : setShowPicker(true)}><Icon>↻</Icon><span>Restore</span></button>
      <div className="rail-spacer"/>
      <SettingsMenu open={showSettings} onToggle={() => setShowSettings(v => !v)}/>
    </aside>
    <main className="center-panel">
      {p.snapshots.length === 0 ? <div className="mission-empty"><div className="empty-mark">□</div><h1>Save your first workspace</h1><p>PC Snapshot remembers your open apps, windows, tabs and terminal — so you can bring the whole setup back in one click. Everything stays on this PC.</p><button className="primary" onClick={p.onCapture}>◉ Capture my desktop <kbd>Ctrl S</kbd></button><button className="link" onClick={p.onImport}>or import snapshots from a backup</button></div> : <>
        <div className="grid-header"><h1>All snapshots <small>{p.snapshots.length}</small></h1><div className="search">⌕ <input ref={searchRef} value={search} onChange={e => setSearch(e.target.value)} placeholder="Search or filter"/></div></div>
        <div className="snapshot-grid">{filtered.map(s => <article key={s.id} className={`snapshot-card ${p.selectedId === s.id ? "selected" : ""} ${s.warning_count ? "has-warning" : "ok"}`} onClick={() => p.onSelect(p.selectedId === s.id ? null : s.id)}>
          <div className="thumb">{s.thumbnail_path && <img src={convertFileSrc(s.thumbnail_path)} alt=""/>}<div className="card-actions"><button onClick={e => {e.stopPropagation(); p.onRestore(s.id)}}>Restore</button><button onClick={e => {e.stopPropagation(); p.onRecapture(s.id)}}>↻</button><button onClick={e => {e.stopPropagation(); p.onDelete(s.id)}}>×</button></div></div>
          <div className="card-copy"><strong>{s.name}</strong>{p.activeSessionId === s.id
            ? <span className="working">● <i>Currently working</i></span>
            : s.warning_count
              ? <span className="warn">● <i>{s.warning_count} warnings</i></span>
              : <span className="good">● <i>{relative(s.timestamp)}</i></span>}</div>
        </article>)}</div>
      </>}
    </main>
    <aside className={`right-panel ${p.selectedId ? "show-details" : ""}`}>
      <div className="resizer" onMouseDown={onDragStart}/>
      <section className="panel-page activity"><div className="panel-title"><span><b className="good">●</b> Activity</span></div><div className="event-list">{p.events.length === 0 ? <p className="muted">Actions you take will appear here.</p> : p.events.map(e => <div className="event" key={e.id}><div className="event-meta">— {e.kind.replace("_", " ")} · {relative(e.timestamp)} —</div><strong className={e.status}>{e.status === "success" ? "✓" : "!"} {e.summary}</strong>{e.detail_lines.map((d,i) => <p key={i}>› {d}</p>)}</div>)}</div></section>
      <section className="panel-page details">
        <div className="detail-scroll">
          <button className="back" onClick={() => p.onSelect(null)}>← Activity</button>
          <div className="detail-preview">{selected?.thumbnail_path && <img src={convertFileSrc(selected.thumbnail_path)} alt=""/>}<span>preview · {new Set(details?.windows.map(w => w.monitor_index)).size || 1} monitors</span></div>
          <h2>{selected?.name}</h2>
          <p className="muted">Captured {selected ? relative(selected.timestamp).toLowerCase() : ""}</p>
          <p className={selected?.warning_count ? "warning-text" : "success-text"}>● {selected?.warning_count ? `Captured with ${selected.warning_count} warning${selected.warning_count === 1 ? "" : "s"}` : "Captured successfully"}</p>
          {details && details.warnings.length > 0 && <div className="snapshot-warnings" role="status" aria-label="Capture warnings">
            <div className="warning-heading">Warning details</div>
            {details.warnings.map((warning, index) => <div className="warning-message" key={`${index}-${warning}`}><span>!</span><p>{warning}</p></div>)}
          </div>}
          <div className="contents-head"><span>CONTENTS</span><span>{details?.processes.length ?? "…"} apps</span></div>
          {details?.processes.map(proc => <div className="app-row" key={`${proc.pid}-${proc.name}`}><AppIcon proc={proc}/><b>{proc.name.replace(/\.exe$/i, "")}</b><small>{details.windows.filter(w => w.exe_path?.toLowerCase().includes(proc.name.replace(/\.exe$/i,"" ).toLowerCase())).length || ""}</small></div>)}
        </div>
        <div className="detail-actions"><button className="primary" onClick={() => p.selectedId && p.onRestore(p.selectedId)}>↻ Restore</button><button onClick={() => p.selectedId && p.onRecapture(p.selectedId)}>↻</button><button className="danger" aria-label="Delete" onClick={() => p.selectedId && p.onDelete(p.selectedId)}><svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"><polyline points="3 6 5 6 21 6"/><path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6m3 0V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2"/><line x1="10" y1="11" x2="10" y2="17"/><line x1="14" y1="11" x2="14" y2="17"/></svg></button></div>
      </section>
    </aside>
    </>}
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
              <span className="thumb-sm">{s.thumbnail_path && <img src={convertFileSrc(s.thumbnail_path)} alt=""/>}</span>
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
