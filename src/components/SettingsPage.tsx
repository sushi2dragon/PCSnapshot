import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { convertFileSrc } from "@tauri-apps/api/core";
import { useIgnoreList } from "../hooks/useIgnoreList";
import { useClipboard } from "../hooks/useClipboard";
import { useCompanion } from "../hooks/useCompanion";
import { openExternal } from "../commands/config";
import { KebabMenu } from "./KebabMenu";
import type { ClipboardCacheRow, CompanionBrowser, SnapshotSummary } from "../types/snapshot";

/** Placeholder for the eventual web-store listing. */
const EXTENSION_INSTALL_URL = "https://google.com";

type Section = "general" | "optins" | "ignore" | "capture" | "terminal" | "companion" | "clipboard" | "storage" | "transfer" | "account" | "about";

/** Detail pages that only exist while their opt-in is on; their nav entries and
 *  sections float into place when revealed. */
const OPTIN_DETAIL = new Set<Section>(["terminal", "companion", "clipboard"]);

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

function Toggle({ checked, label, onClick }: { checked: boolean; label: string; onClick: () => void }) {
  return <button className={`settings-toggle ${checked ? "on" : ""}`} role="switch" aria-checked={checked} aria-label={label} onClick={onClick}><span /></button>;
}

function SettingRow({ title, description, action }: { title: string; description: string; action?: React.ReactNode }) {
  return <div className="setting-row"><div><strong>{title}</strong><p>{description}</p></div>{action && <div className="setting-row-action">{action}</div>}</div>;
}

const titleCase = (s: string) => (s ? s.charAt(0).toUpperCase() + s.slice(1) : s);

/** Short host label for a tab URL (drops protocol and leading www.). */
function tabHost(url: string): string {
  try { return new URL(url).hostname.replace(/^www\./, ""); } catch { return ""; }
}

function formatWhen(iso: string): string {
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return "recently";
  return d.toLocaleString(undefined, { month: "short", day: "numeric", hour: "numeric", minute: "2-digit" });
}

/** Connected/known browsers plus each one's most recently captured tabs — the
 *  body of the Browser Companion settings page. */
function CompanionBrowsers({ browsers, loading }: { browsers: CompanionBrowser[]; loading: boolean }) {
  if (loading && browsers.length === 0) return <div className="settings-empty">Checking connected browsers…</div>;
  if (browsers.length === 0) return <div className="settings-empty">No browser has connected yet. Install the extension and open your browser, then re-check.</div>;
  return <>{browsers.map(b => (
    <div className="companion-browser" key={b.family}>
      <div className="companion-browser-head">
        <div className="app-glyph">{b.family.slice(0, 2).toUpperCase()}</div>
        <div className="companion-browser-copy">
          <strong>{titleCase(b.family)}</strong>
          <span>
            {b.connected ? "Connected now" : b.last_captured_at ? `Last captured ${formatWhen(b.last_captured_at)}` : "Registered"}
            {b.tab_count ? ` · ${b.tab_count} tab${b.tab_count === 1 ? "" : "s"}` : ""}
          </span>
        </div>
        {b.connected && <span className="companion-dot" title="Connected now" />}
      </div>
      {b.tabs.length > 0 && <ul className="companion-tabs">
        {b.tabs.map((t, i) => <li key={i}>
          <span className="companion-tab-title">{t.title || tabHost(t.url) || "(untitled tab)"}</span>
          <span className="companion-tab-url">{tabHost(t.url)}</span>
        </li>)}
      </ul>}
    </div>
  ))}</>;
}

export function SettingsPage(p: Props) {
  const [activeSection, setActiveSection] = useState<Section>("general");
  // The overlay owns its own dismissal so the exit animation can play out before
  // the parent unmounts it. Every close path (× button, backdrop, Escape) routes here.
  const [closing, setClosing] = useState(false);
  const onCloseRef = useRef(p.onClose);
  onCloseRef.current = p.onClose;
  const close = useCallback(() => {
    setClosing(true);
    window.setTimeout(() => onCloseRef.current(), 260);
  }, []);
  const contentRef = useRef<HTMLDivElement>(null);
  const [ignoreQuery, setIgnoreQuery] = useState("");
  const [pickerQuery, setPickerQuery] = useState("");
  const [pickerOpen, setPickerOpen] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const { list, running, loading, add, remove, refresh } = useIgnoreList();
  const clip = useClipboard();
  // Browser companion is connection-driven, not a toggle: its detail page appears
  // once a browser has connected (or was captured before). `install` sends the
  // user to the extension listing; `companion.refresh` re-checks the live status.
  const companion = useCompanion();
  const installCompanion = useCallback(() => { void openExternal(EXTENSION_INSTALL_URL).catch(() => {}); }, []);

  // Nav + scroll-spy are driven off this list. The three opt-in detail pages are
  // present only while their feature is active, so toggling one (or a browser
  // connecting) mounts/unmounts both its nav entry and its section together.
  const sections = useMemo<{ key: Section; label: string }[]>(() => [
    { key: "general", label: "General" },
    { key: "optins", label: "Opt-Ins" },
    { key: "ignore", label: "Ignore List" },
    { key: "capture", label: "Capture" },
    ...(p.terminalHookEnabled ? [{ key: "terminal" as Section, label: "Terminal" }] : []),
    ...(companion.active ? [{ key: "companion" as Section, label: "Browser Companion" }] : []),
    ...(clip.enabled ? [{ key: "clipboard" as Section, label: "Clipboard Cache" }] : []),
    { key: "storage", label: "Storage" },
    { key: "transfer", label: "Import & Export" },
    { key: "account", label: "Plans & Account" },
    { key: "about", label: "About & Help" },
  ], [p.terminalHookEnabled, companion.active, clip.enabled]);

  // If the active page is an opt-in that was just turned off, its nav entry is
  // gone; fall back to the Opt-Ins hub so the highlight never points at nothing.
  useEffect(() => {
    if (!sections.some(s => s.key === activeSection)) setActiveSection("optins");
  }, [sections, activeSection]);
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

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== "Escape") return;
      if (pickerOpen) setPickerOpen(false); else close();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [pickerOpen, close]);

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

  return createPortal(<div className={`settings-overlay ${closing ? "closing" : ""}`} role="dialog" aria-modal="true" aria-label="Settings"
    onMouseDown={e => e.target === e.currentTarget && close()}>
    <section className="settings-page">
    <button className="settings-close" aria-label="Close settings" onClick={close}>×</button>
    <nav className="settings-nav" aria-label="Settings sections">
      <div className="settings-nav-head"><span>Settings</span></div>
      {sections.map(item => <button key={item.key} className={`${activeSection === item.key ? "active" : ""}${OPTIN_DETAIL.has(item.key) ? " nav-reveal" : ""}`} aria-current={activeSection === item.key ? "location" : undefined} onClick={() => jumpToSection(item.key)}>{item.label}</button>)}
    </nav>
    <div className="settings-content" ref={contentRef} onScroll={trackVisibleSection}>
      <section className="settings-section" id="settings-general">
        <header className="settings-heading"><div><h1>General</h1><p>PC Snapshot stays local, focused, and ready when you need it.</p></div></header>
        <div className="settings-card"><SettingRow title="Local-first storage" description="Snapshots, thumbnails, and activity remain on this PC. No account or cloud connection is used."/><SettingRow title="Refresh library" description="Reload snapshot metadata from local storage." action={<button className="settings-secondary" onClick={p.onRefresh}>Refresh now</button>}/></div>
      </section>

      <section className="settings-section" id="settings-optins">
        <header className="settings-heading"><div><h1>Opt-Ins</h1><p>Optional features are off by default. Turn one on and its own settings page slides into the sidebar.</p></div></header>
        <div className="settings-card">
          <SettingRow title="Terminal directory capture" description="Adds a small PowerShell profile hook so terminal working directories can be restored." action={<Toggle checked={p.terminalHookEnabled} label="Terminal directory capture" onClick={p.onToggleTerminalHook}/>}/>
          <SettingRow title="Browser companion" description="Capture and restore exact browser tabs through the PC Snapshot extension." action={
            companion.connected
              ? <span className="companion-badge"><svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.6" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true"><polyline points="20 6 9 17 4 12"/></svg>Connected</span>
              : <div className="companion-actions">
                  <button className="settings-primary" onClick={installCompanion}>Install extension</button>
                  <button className="settings-linkish" disabled={companion.loading} onClick={() => { void companion.refresh(); }}>{companion.loading ? "Checking…" : "Recheck"}</button>
                </div>
          }/>
          <SettingRow title="Clipboard cache" description="Store the clipboard and Win+V history with each snapshot and reseed it on restore." action={<Toggle checked={clip.enabled} label="Clipboard cache" onClick={() => { void clip.toggle(); }}/>}/>
        </div>
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

      {p.terminalHookEnabled && <section className="settings-section settings-reveal" id="settings-terminal">
        <header className="settings-heading"><div><h1>Terminal</h1><p>PowerShell working directories are recorded so terminals reopen where you left them.</p></div></header>
        <div className="settings-card"><SettingRow title="PowerShell directory capture" description="A small PowerShell profile hook records each terminal's working directory. Turning this off removes the hook and hides this page." action={<Toggle checked={p.terminalHookEnabled} label="PowerShell directory capture" onClick={p.onToggleTerminalHook}/>}/></div>
      </section>}

      {companion.active && <section className="settings-section settings-reveal" id="settings-companion">
        <header className="settings-heading">
          <div><h1>Browser Companion</h1><p>Browsers connected through the PC Snapshot extension, with the tabs from their most recent capture.</p></div>
          <button className="settings-secondary" disabled={companion.loading} onClick={() => { void companion.refresh(); }}>{companion.loading ? "Checking…" : "Check again"}</button>
        </header>
        {companion.report && !companion.report.host_installed && <div className="settings-error">The companion relay is missing at {companion.report.host_path}. Reinstall PC Snapshot to restore browser tabs.</div>}
        <div className="companion-list">
          <CompanionBrowsers browsers={companion.browsers} loading={companion.loading}/>
        </div>
      </section>}

      {clip.enabled && <section className="settings-section settings-reveal" id="settings-clipboard">
        <header className="settings-heading"><div><h1>Clipboard Cache</h1><p>The clipboard and Win+V history are captured with each snapshot and reseeded on restore. Passwords marked sensitive by their app are never captured; pinned Win+V items are left untouched.</p></div></header>
        {/* Bounded, self-scrolling panel: the cache grows with every snapshot and
            backup, so an unbounded list would push the rest of Settings off-screen. */}
        <div className="clip-cache-panel">
        <div className="clip-cache-head"><span>Saved items</span>{!clip.loading && <span>{clip.rows.length} item{clip.rows.length === 1 ? "" : "s"}</span>}</div>
        <div className="clip-cache">
          {clip.loading ? <div className="settings-empty">Loading clipboard items…</div>
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
        <div className="settings-card"><SettingRow title="Capture clipboard" description="Turning this off stops all clipboard capture and reseeding and hides this page. Nothing is read or written while off." action={<Toggle checked={clip.enabled} label="Capture clipboard" onClick={() => { void clip.toggle(); }}/>}/></div>
      </section>}

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
    </section>
  </div>, document.body);
}
