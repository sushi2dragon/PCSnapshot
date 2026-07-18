import { useMemo, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { useIgnoreList } from "../hooks/useIgnoreList";
import type { SnapshotSummary } from "../types/snapshot";

type Section = "general" | "ignore" | "capture" | "terminal" | "storage" | "transfer" | "account" | "about";

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

export function SettingsPage(p: Props) {
  const [activeSection, setActiveSection] = useState<Section>("general");
  const contentRef = useRef<HTMLDivElement>(null);
  const [query, setQuery] = useState("");
  const [pickerOpen, setPickerOpen] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const { list, running, loading, add, remove, refresh } = useIgnoreList();
  const filteredIgnored = useMemo(() => list.filter(x => x.toLowerCase().includes(query.toLowerCase())), [list, query]);
  const available = useMemo(() => running.filter(x => !list.includes(x) && x.toLowerCase().includes(query.toLowerCase())), [running, list, query]);

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
        <div className="settings-toolbar"><div className="settings-search">⌕<input value={query} onChange={e => setQuery(e.target.value)} placeholder="Search applications…" /></div><button className="settings-primary" onClick={() => setPickerOpen(true)}>+ Add app</button></div>
        {error && <div className="settings-error">{error}</div>}
        <div className="ignore-list">
          {loading ? <div className="settings-empty">Loading applications…</div> : filteredIgnored.length === 0 ? <div className="settings-empty">{query ? "No ignored apps match your search." : "No apps are ignored yet."}</div> : filteredIgnored.map(stem => <div className="ignore-row" key={stem}>
            <div className="app-glyph">{stem.slice(0, 2).toUpperCase()}</div><div className="ignore-copy"><strong>{stem}</strong><span>Added by you · excluded from capture and restore</span></div>
            <Toggle checked label={`Stop ignoring ${stem}`} onClick={() => remove(stem)} />
            <button className="remove-ignore" aria-label={`Remove ${stem}`} onClick={() => remove(stem)}>×</button>
          </div>)}
        </div>
        {pickerOpen && createPortal(<div className="settings-picker" role="dialog" aria-modal="true" aria-label="Add an app to Ignore List" onMouseDown={e => e.target === e.currentTarget && setPickerOpen(false)}>
          <div className="settings-picker-card"><div className="settings-picker-head"><div><h2>Add an app</h2><p>Select a currently running application.</p></div><button aria-label="Close" onClick={() => setPickerOpen(false)}>×</button></div>
            <div className="settings-search">⌕<input autoFocus value={query} onChange={e => setQuery(e.target.value)} placeholder="Search running apps…" /></div>
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
        <div className="settings-card"><SettingRow title="PowerShell directory capture" description="Adds a small PowerShell profile hook so terminal working directories can be restored." action={<Toggle checked={p.terminalHookEnabled} label="PowerShell directory capture" onClick={p.onToggleTerminalHook}/>}/><SettingRow title="Browser companion" description="Browser tabs are captured when the local companion is connected; failures remain non-fatal."/></div>
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
