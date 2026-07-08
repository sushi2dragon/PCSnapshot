import { useState, useRef, useEffect } from "react";

interface SettingsMenuProps {
  onClearAll: () => void;
  onImport: () => void;
  onHelp: () => void;
  onIgnoreList: () => void;
}

const menuItems = [
  {
    key: "ignoreList",
    label: "Ignore List",
    description: "Exclude apps from snapshots",
    danger: false,
    icon: (
      <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
        <path d="M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10z" />
      </svg>
    ),
  },
  {
    key: "clearAll",
    label: "Clear All",
    description: "Delete all snapshots",
    danger: true,
    icon: (
      <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
        <polyline points="3 6 5 6 21 6" />
        <path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6m3 0V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2" />
      </svg>
    ),
  },
  {
    key: "import",
    label: "Import",
    description: "Import snapshots from folder",
    danger: false,
    icon: (
      <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
        <path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4" />
        <polyline points="17 8 12 3 7 8" />
        <line x1="12" y1="3" x2="12" y2="15" />
      </svg>
    ),
  },
  {
    key: "help",
    label: "Help",
    description: "Keyboard shortcuts & tips",
    danger: false,
    icon: (
      <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
        <circle cx="12" cy="12" r="10" />
        <path d="M9.09 9a3 3 0 0 1 5.83 1c0 2-3 3-3 3" />
        <line x1="12" y1="17" x2="12.01" y2="17" />
      </svg>
    ),
  },
];

export function SettingsMenu({ onClearAll, onImport, onHelp, onIgnoreList }: SettingsMenuProps) {
  const [open, setOpen] = useState(false);
  const [confirmClear, setConfirmClear] = useState(false);
  const menuRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    const handler = (e: MouseEvent) => {
      if (menuRef.current && !menuRef.current.contains(e.target as Node)) {
        setOpen(false);
        setConfirmClear(false);
      }
    };
    document.addEventListener("mousedown", handler);
    return () => document.removeEventListener("mousedown", handler);
  }, [open]);

  const handleClearAll = () => {
    if (confirmClear) {
      onClearAll();
      setOpen(false);
      setConfirmClear(false);
    } else {
      setConfirmClear(true);
    }
  };

  return (
    <div className="relative" ref={menuRef}>
      {/* Settings text button */}
      <button
        onClick={() => {
          setOpen((v) => !v);
          setConfirmClear(false);
        }}
        className="flex items-center justify-center rounded-md cursor-pointer transition-colors px-4 py-2 text-sm font-medium"
        style={{
          backgroundColor: open ? "var(--bg-tile-hover)" : "var(--bg-tile)",
          color: "var(--text-secondary)",
          width: "100%",
        }}
        onMouseEnter={(e) => (e.currentTarget.style.backgroundColor = "var(--bg-tile-hover)")}
        onMouseLeave={(e) => (e.currentTarget.style.backgroundColor = open ? "var(--bg-tile-hover)" : "var(--bg-tile)")}
      >
        Settings
      </button>

      {/* Dropdown menu */}
      {open && (
        <div
          className="absolute right-0 rounded-lg overflow-hidden z-50"
          style={{
            top: "calc(100% + 6px)",
            minWidth: 200,
            backgroundColor: "var(--bg-card)",
            border: "1px solid var(--border-subtle)",
            boxShadow: "0 8px 24px rgba(0,0,0,0.4)",
          }}
        >
          {menuItems.map((item, i) => {
            const isClearAll = item.key === "clearAll";
            const isConfirming = isClearAll && confirmClear;

            return (
              <button
                key={item.key}
                onClick={() => {
                  if (isClearAll) { handleClearAll(); return; }
                  if (item.key === "ignoreList") { onIgnoreList(); setOpen(false); }
                  if (item.key === "import") { onImport(); setOpen(false); }
                  if (item.key === "help") { onHelp(); setOpen(false); }
                }}
                className="w-full flex items-center gap-3 px-4 py-3 text-left cursor-pointer transition-colors"
                style={{
                  borderTop: i > 0 ? "1px solid var(--border-subtle)" : "none",
                  backgroundColor: isConfirming ? "rgba(220,38,38,0.1)" : "transparent",
                  color: isConfirming ? "#f87171" : item.danger ? "#f87171" : "var(--text-primary)",
                }}
                onMouseEnter={(e) => {
                  if (!isConfirming)
                    e.currentTarget.style.backgroundColor = "var(--bg-tile)";
                }}
                onMouseLeave={(e) => {
                  e.currentTarget.style.backgroundColor = isConfirming
                    ? "rgba(220,38,38,0.1)"
                    : "transparent";
                }}
              >
                <span style={{ color: isConfirming ? "#f87171" : item.danger ? "#f87171" : "var(--text-secondary)" }}>
                  {item.icon}
                </span>
                <div>
                  <p className="text-xs font-medium leading-tight">
                    {isConfirming ? "Click again to confirm" : item.label}
                  </p>
                  {!isConfirming && (
                    <p className="text-xs leading-tight mt-0.5" style={{ color: "var(--text-secondary)" }}>
                      {item.description}
                    </p>
                  )}
                </div>
              </button>
            );
          })}
        </div>
      )}
    </div>
  );
}
