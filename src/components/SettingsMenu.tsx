interface SettingsMenuProps {
  open?: boolean;
  onToggle?: () => void;
  /** Legacy grid fallback; MissionControl supplies onToggle. */
  onIgnoreList?: () => void;
  onClearAll?: () => void;
  onImport?: () => void;
  onHelp?: () => void;
  onToggleTerminalHook?: () => void;
  terminalHookEnabled?: boolean;
}
export function SettingsMenu({ open = false, onToggle, onIgnoreList }: SettingsMenuProps) {
  return (
    <div className="relative">
      <button
        onClick={onToggle ?? onIgnoreList}
        className="settings-trigger flex items-center justify-center rounded-md cursor-pointer transition-colors"
        aria-label="Settings"
        title="Settings"
        style={{
          backgroundColor: open ? "var(--bg-tile-hover)" : "var(--bg-tile)",
          color: "var(--text-secondary)",
          width: "100%",
        }}
        onMouseEnter={(e) => (e.currentTarget.style.backgroundColor = "var(--bg-tile-hover)")}
        onMouseLeave={(e) => (e.currentTarget.style.backgroundColor = open ? "var(--bg-tile-hover)" : "var(--bg-tile)")}
      >
        <svg aria-hidden="true" width="25" height="25" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round">
          <circle cx="12" cy="12" r="3" />
          <path d="M19.4 15a1.7 1.7 0 0 0 .34 1.88l.06.06-2.83 2.83-.06-.06a1.7 1.7 0 0 0-1.88-.34 1.7 1.7 0 0 0-1.03 1.56V21h-4v-.08A1.7 1.7 0 0 0 8.94 19.4a1.7 1.7 0 0 0-1.88.34l-.06.06-2.83-2.83.06-.06A1.7 1.7 0 0 0 4.57 15 1.7 1.7 0 0 0 3 14H3v-4h.08A1.7 1.7 0 0 0 4.6 8.94a1.7 1.7 0 0 0-.34-1.88L4.2 7l2.83-2.83.06.06A1.7 1.7 0 0 0 8.97 4.6 1.7 1.7 0 0 0 10 3.08V3h4v.08a1.7 1.7 0 0 0 1.03 1.56 1.7 1.7 0 0 0 1.88-.34l.06-.06L19.8 7l-.06.06a1.7 1.7 0 0 0-.34 1.88A1.7 1.7 0 0 0 20.92 10H21v4h-.08A1.7 1.7 0 0 0 19.4 15Z" />
        </svg>
      </button>
    </div>
  );
}
