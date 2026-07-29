import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { convertFileSrc } from "@tauri-apps/api/core";
import { useIgnoreList } from "../hooks/useIgnoreList";
import { useClipboard } from "../hooks/useClipboard";
import { companionStatus, refreshCompanion } from "../commands/snapshots";
import { KebabMenu } from "./KebabMenu";
import type { ClipboardCacheRow, CompanionReport, SnapshotSummary } from "../types/snapshot";

type Section = "general" | "ignore" | "capture" | "terminal" | "clipboard" | "storage" | "transfer" | "account" | "about";

type Props = {
  snapshots: SnapshotSummary[];
  terminalHookEnabled: boolean;
  onToggleTerminalHook: () => void;
  onClearAll: () => void;
  onImport: () => void;
  onHelp: () => void;
  onRefresh: () => void;
  onClose: () => void;
};

const sections: { key: Section; label: string }[] = [
  { key: "general", label: "General" },
  { key: "ignore", label: "Ignore List" },
  { key: "capture", label: "Capture" },
  { key: "terminal", label: "Terminal & Browser" },
  { key: "clipboard", label: "Clipboard Cache" },
  { key: "storage", label: "Storage" },
  { key: "transfer", label: "Import & Export" },
  { key: "account", label: "Plans & Account" },
  { key: "about", label: "About & Help" },
];

function Toggle({ checked, label, onClick }: { checked: boolean; label: string; onClick: () => void }) {
  return <button className={`settings-toggle ${checked ? "on" : ""}`} role="switch" aria-checked={checked} aria-label={label} onClick={onClick}><span /></button>;
}

function SettingRow({ title, description, action }: { title: string; description: string; action?: React.ReactNode }) {
  return <div className="setting-row"><div><strong>{title}</strong><p>{description}</p></div>{action && <div className="setting-row-action">{action}</div>}</div>;
}

/**
 * Live companion state. The companion used to fail silently — a stale native-host
 * registration or a service worker the browser had shut down both surfaced only
 * as a warning buried in a restore report. This row answers "is it working right
 * now?" directly, and re-runs setup on demand.
 */
function CompanionRow() {
  const [report, setReport] = useState<CompanionReport | null>(null);
  const [checking, setChecking] = useState(true);

  useEffect(() => {
    let alive = true;
    companionStatus()
      .then(next => { if (alive) setReport(next); })
      .catch(() => {})
      .finally(() => { if (alive) setChecking(false); });
    return () => { alive = false; };
  }, []);

  const recheck = useCallback(async () => {
    setChecking(true);
    try {
      setReport(await refreshCompanion());
    } catch {
      setReport(null);
    } finally {
      setChecking(false);
    }
  }, []);

  const description = !report
    ? "Checking the browser companion…"
    : !report.host_installed
      ? `The companion relay is missing at ${report.host_path}. Reinstall PC Snapshot to restore browser tabs.`
      : report.connected_browsers.length > 0
        ? `Connected to ${report.connected_browsers.join(", ")}. Browser tabs are captured and restored exactly.`
        : "No browser is connected. Install the PC Snapshot Companion extension and open that browser; setup is registered for "
          + `${report.registered_browsers.join(", ") || "no browsers"}.`;

  return <SettingRow
    title="Browser companion"
    description={description}
    action={<button className="settings-linkish" disabled={checking} onClick={() => { void recheck(); }}>
      {checking ? "Checking…" : "Check again"}
    </button>}
  />;
}

export function SettingsPage(p: Props) {
  const [activeSection, setActiveSection] = useState<Section>("general");
  const contentRef = useRef<HTMLDivElement>(null);
  const [ignoreQuery, setIgnoreQuery] = useState("");
  const [pickerQuery, setPickerQuery] = useState("");
  const [pickerOpen, setPickerOpen] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const { list, running, loading, add, remove, refresh } = useIgnoreList();
  const clip = useClipboard();
  // Copy feedback: the click is otherwise silent, and a silent button reads as
  // a broken one. Re-arms on every press so repeat copies each show a tick.
  const [copied, setCopied] = useState<{ id: string; ok: boolean } | null>(null);
  const copyTimer = useRef<number | null>(null);
  const copyRow = (row: ClipboardCacheRow) => {
    const settle = (ok: boolean) => {
      if (copyTimer.current !== null) window.clearTimeout(copyTimer.current);
      setCopied({ id: row.row_id, ok });
      copyTimer.current = window.setTimeout(() => { copyTimer.current = null; setCopied(null); }, ok ? 1400 : 2600);
    };
    clip.copy(row).then(() => settle(true)).catch(() => settle(false));
  };
  const filteredIgnored = useMemo(() => list.filter(x => x.toLowerCase().includes(ignoreQuery.toLowerCase())), [list, ignoreQuery]);
  const available = useMemo(() => running.filter(x => !list.includes(x) && x.toLowerCase().includes(pickerQuery.toLowerCase())), [running, list, pickerQuery]);

  const addApp = async (name: string) => {
    try { await add(name); setError(null); setPickerOpen(false); }
    catch (e) { setError(String(e)); }
  };

  const jumpToSection = (key: Section) => {
    const target = contentRef.current?.querySelector<HTMLElement>(`#settings-${key}`);
    if (!target) return;
    setActiveSection(key);
    target.scrollIntoView({ behavior: "smooth", block: "start" });
  };

  const trackVisibleSection = () => {
    const content = contentRef.current;
    if (!content) return;
    if (content.scrollTop + content.clientHeight >= content.scrollHeight - 2) {
      setActiveSection(sections[sections.length - 1].key);
      return;
    }
    const marker = content.scrollTop + 72;
    let visible = sections[0].key;
    for (const item of sections) {
      const element = content.querySelector<HTMLElement>(`#settings-${item.key}`);
      if (element && element.offsetTop <= marker) visible = item.key;
    }
    setActiveSection(visible);
  };

  return <section className="settings-page" aria-label="Settings">
    <nav className="settings-nav" aria-label="Settings sections">
      <div className="settings-nav-head"><span>Settings</span><button aria-label="Close settings" onClick={p.onClose}>×</button></div>
      {sections.map(item => <button key={item.key} className={activeSection === item.key ? "active" : ""} aria-current={activeSection === item.key ? "location" : undefined} onClick={() => jumpToSection(item.key)}>{item.label}</button>)}
    </nav>
    <div className="settings-content" ref={contentRef} onScroll={trackVisibleSection}>
      <section className="settings-section" id="settings-general">
        <header className="settings-heading"><div><h1>General</h1><p>PC Snapshot stays local, focused, and ready when you need it.</p></div></header>
        <div className="settings-card"><SettingRow title="Local-first storage" description="Snapshots, thumbnails, and activity remain on this PC. No account or cloud connection is used."/><SettingRow title="Refresh library" description="Reload snapshot metadata from local storage." action={<button className="settings-secondary" onClick={p.onRefresh}>Refresh now</button>}/></div>
      </section>

      <section className="settings-section" id="settings-ignore">
        <header className="settings-heading"><div><h1>Ignore List</h1><p>Apps here are never captured, restored, or closed — useful for background utilities and personal apps.</p></div></header>
        <div className="settings-toolbar"><div className="settings-search">⌕<input value={ignoreQuery} onChange={e => setIgnoreQuery(e.target.value)} placeholder="Search applications…" /></div><button className="settings-primary" onClick={() => { setPickerQuery(""); setPickerOpen(true); }}>+ Add app</button></div>
        {error && <div className="settings-error">{error}</div>}
        <div className="ignore-list">
          {loading ? <div className="settings-empty">Loading applications…</div> : filteredIgnored.length === 0 ? <div className="settings-empty">{ignoreQuery ? "No ignored apps match your search." : "No apps are ignored yet."}</div> : filteredIgnored.map(stem => <div className="ignore-row" key={stem}>
            <div className="app-glyph">{stem.slice(0, 2).toUpperCase()}</div><div className="ignore-copy"><strong>{stem}</strong><span>Added by you · excluded from capture and restore</span></div>
            <Toggle checked label={`Stop ignoring ${stem}`} onClick={() => remove(stem)} />
            <button className="remove-ignore" aria-label={`Remove ${stem}`} onClick={() => remove(stem)}>×</button>
          </div>)}
        </div>
        {pickerOpen && createPortal(<div className="settings-picker" role="dialog" aria-modal="true" aria-label="Add an app to Ignore List" onMouseDown={e => e.target === e.currentTarget && setPickerOpen(false)}>
          <div className="settings-picker-card"><div className="settings-picker-head"><div><h2>Add an app</h2><p>Select a currently running application.</p></div><button aria-label="Close" onClick={() => setPickerOpen(false)}>×</button></div>
            <div className="settings-search">⌕<input autoFocus value={pickerQuery} onChange={e => setPickerQuery(e.target.value)} placeholder="Search running apps…" /></div>
            <div className="running-list">{available.length === 0 ? <div className="settings-empty">No matching running applications.</div> : available.map(stem => <button key={stem} onClick={() => addApp(stem)}><span className="app-glyph">{stem.slice(0,2).toUpperCase()}</span><b>{stem}</b><span>＋</span></button>)}</div>
            <div className="settings-picker-foot"><button onClick={() => refresh()}>Refresh list</button><button onClick={() => setPickerOpen(false)}>Cancel</button></div>
          </div>
        </div>, document.body)}
      </section>

      <section className="settings-section" id="settings-capture">
        <header className="settings-heading"><div><h1>Capture</h1><p>How PC Snapshot records the current desktop.</p></div></header>
        <div className="settings-card"><SettingRow title="Parallel capture" description="Screenshots and window enumeration run together to keep capture under the three-second target."/><SettingRow title="Partial captures" description="A snapshot is still saved when one source fails; the exact warning is shown in Details."/></div>
      </section>

      <section className="settings-section" id="settings-terminal">
        <header className="settings-heading"><div><h1>Terminal & Browser</h1><p>Control optional context collection for richer restores.</p></div></header>
        <div className="settings-card"><SettingRow title="PowerShell directory capture" description="Adds a small PowerShell profile hook so terminal working directories can be restored." action={<Toggle checked={p.terminalHookEnabled} label="PowerShell directory capture" onClick={p.onToggleTerminalHook}/>}/><CompanionRow/></div>
      </section>

      <section className="settings-section" id="settings-clipboard">
        <header className="settings-heading"><div><h1>Clipboard Cache</h1><p>Optionally capture the clipboard and Win+V history with each snapshot, then reseed it on restore. Off means nothing is ever read or written — pinned Win+V items are always left untouched.</p></div></header>
        <div className="settings-card"><SettingRow title="Capture clipboard" description="Store the current clipboard and Win+V history (text and images) inside snapshots, and reseed Win+V when you restore. Passwords marked sensitive by their app are never captured." action={<Toggle checked={clip.enabled} label="Capture clipboard" onClick={() => { void clip.toggle(); }}/>}/></div>
        {/* Bounded, self-scrolling panel: the cache grows with every snapshot and
            backup, so an unbounded list would push the rest of Settings off-screen. */}
        <div className={`clip-cache-panel ${clip.enabled ? "" : "clip-cache-disabled"}`}>
        <div className="clip-cache-head"><span>Saved items</span>{clip.enabled && !clip.loading && <span>{clip.rows.length} item{clip.rows.length === 1 ? "" : "s"}</span>}</div>
        <div className="clip-cache">
          {!clip.enabled ? <div className="settings-empty">Turn on clipboard capture to browse and re-copy saved clipboard items.</div>
            : clip.loading ? <div className="settings-empty">Loading clipboard items…</div>
            : clip.rows.length === 0 ? <div className="settings-empty">No clipboard items captured yet.</div>
            : clip.rows.map(row => <div className="clip-cache-row" key={row.row_id}>
                {row.kind === "image" && row.sidecar_path
                  ? <img className="clip-thumb" src={convertFileSrc(row.sidecar_path)} alt=""/>
                  : <span className="clip-glyph">T</span>}
                <div className="clip-cache-copy"><strong>{row.kind === "image" ? "Image" : ((row.text ?? "").trim().slice(0, 120) || "(empty)")}</strong><span>{row.label}</span></div>
                {(() => {
                  const state = copied?.id === row.row_id ? (copied.ok ? "copied" : "copy-failed") : "";
                  return <button className={`settings-secondary clip-cache-btn ${state}`} onClick={() => copyRow(row)}
                    title={state === "copy-failed" ? "Copy failed — Windows did not accept the clipboard write" : "Copy to clipboard"}>
                    {state === "copied"
                      ? <><svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.6" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true"><polyline points="20 6 9 17 4 12"/></svg>Copied</>
                      : state === "copy-failed" ? "Failed" : "Copy"}
                  </button>;
                })()}
                <KebabMenu items={[{ label: "Copy", onClick: () => copyRow(row) }, { label: "Delete", danger: true, onClick: () => { void clip.remove(row); } }]}/>
              </div>)}
        </div>
        </div>
      </section>

      <section className="settings-section" id="settings-storage">
        <header className="settings-heading"><div><h1>Storage</h1><p>Manage snapshots stored on this PC.</p></div></header>
        <div className="settings-card"><SettingRow title="Saved snapshots" description={`${p.snapshots.length} snapshot${p.snapshots.length === 1 ? "" : "s"} currently stored.`}/><SettingRow title="Delete all snapshots" description="Permanently removes all snapshot JSON files and thumbnails." action={<button className="settings-danger" onClick={p.onClearAll}>Clear all</button>}/></div>
      </section>

      <section className="settings-section" id="settings-transfer">
        <header className="settings-heading"><div><h1>Import & Export</h1><p>Move local snapshots between installations.</p></div></header>
        <div className="settings-card"><SettingRow title="Import snapshots" description="Choose a backup folder containing PC Snapshot files." action={<button className="settings-secondary" onClick={p.onImport}>Import</button>}/><SettingRow title="Export" description="Export support is planned; snapshots currently remain in the local Snapshots data folder."/></div>
      </section>

      <section className="settings-section" id="settings-account">
        <header className="settings-heading"><div><h1>Plans & Account</h1><p>No sign-in required.</p></div></header>
        <div className="settings-card"><SettingRow title="Local edition" description="PC Snapshot has no account, subscription, telemetry profile, or cloud sync."/></div>
      </section>

      <section className="settings-section" id="settings-about">
        <header className="settings-heading"><div><h1>About & Help</h1><p>PC Snapshot 0.1.0</p></div></header>
        <div className="settings-card"><SettingRow title="Keyboard shortcuts" description="Ctrl+S Capture · Ctrl+K Search · Enter Restore · Delete Remove · Escape Back" action={<button className="settings-secondary" onClick={p.onHelp}>Show help</button>}/><SettingRow title="Privacy" description="Everything stays on this PC."/></div>
      </section>
    </div>
  </section>;
}
