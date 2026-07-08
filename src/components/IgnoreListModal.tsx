import { useState, useRef, useEffect } from "react";
import { useIgnoreList } from "../hooks/useIgnoreList";

interface IgnoreListModalProps {
  isOpen: boolean;
  onClose: () => void;
}

export function IgnoreListModal({ isOpen, onClose }: IgnoreListModalProps) {
  const { list, running, loading, add, remove } = useIgnoreList();
  const [input, setInput] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [filter, setFilter] = useState("");
  const inputRef = useRef<HTMLInputElement>(null);

  // The parent remounts this modal (via key) each time it opens, so input/error/
  // filter start fresh and useIgnoreList refetches on mount — no setState in effects.
  useEffect(() => {
    if (isOpen) {
      const t = setTimeout(() => inputRef.current?.focus(), 50);
      return () => clearTimeout(t);
    }
  }, [isOpen]);

  if (!isOpen) return null;

  const handleAdd = async () => {
    const trimmed = input.trim();
    if (!trimmed) return;
    try {
      await add(trimmed);
      setInput("");
      setError(null);
    } catch (e) {
      setError(String(e));
    }
  };

  const handlePickRunning = async (stem: string) => {
    try {
      await add(stem);
      setError(null);
    } catch (e) {
      setError(String(e));
    }
  };

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "Enter") handleAdd();
    if (e.key === "Escape") onClose();
  };

  const filteredRunning = filter
    ? running.filter((s) => s.includes(filter.toLowerCase()))
    : running;

  return (
    <div
      className="fixed inset-0 z-40 flex items-center justify-center"
      style={{ backgroundColor: "rgba(0,0,0,0.6)" }}
      onClick={onClose}
    >
      <div
        className="rounded-xl shadow-2xl p-5 w-96 max-h-[80vh] flex flex-col"
        style={{ backgroundColor: "var(--bg-card)" }}
        onClick={(e) => e.stopPropagation()}
      >
        {/* Header */}
        <div className="flex items-center justify-between mb-4">
          <h2 className="text-sm font-semibold" style={{ color: "var(--text-primary)" }}>
            Ignore List
          </h2>
          <button
            onClick={onClose}
            className="p-1 rounded cursor-pointer transition-colors"
            style={{ color: "var(--text-secondary)" }}
            onMouseEnter={(e) => (e.currentTarget.style.color = "var(--text-primary)")}
            onMouseLeave={(e) => (e.currentTarget.style.color = "var(--text-secondary)")}
          >
            <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
              <line x1="18" y1="6" x2="6" y2="18" />
              <line x1="6" y1="6" x2="18" y2="18" />
            </svg>
          </button>
        </div>

        <p className="text-xs mb-3" style={{ color: "var(--text-secondary)" }}>
          Ignored processes are excluded from capture, restore, and close.
        </p>

        {/* Current entries */}
        {list.length > 0 && (
          <div className="mb-3">
            <p className="text-xs font-medium mb-1.5" style={{ color: "var(--text-secondary)" }}>
              Currently ignored
            </p>
            <div
              className="rounded-md overflow-hidden"
              style={{ border: "1px solid var(--border-subtle)" }}
            >
              {list.map((stem) => (
                <div
                  key={stem}
                  className="flex items-center justify-between px-3 py-2"
                  style={{
                    backgroundColor: "var(--bg-tile)",
                    borderBottom: "1px solid var(--border-subtle)",
                  }}
                >
                  <span className="text-xs font-mono" style={{ color: "var(--text-primary)" }}>
                    {stem}
                  </span>
                  <button
                    onClick={() => remove(stem)}
                    className="text-xs px-2 py-0.5 rounded cursor-pointer transition-colors"
                    style={{ color: "#f87171" }}
                    onMouseEnter={(e) => (e.currentTarget.style.backgroundColor = "rgba(220,38,38,0.1)")}
                    onMouseLeave={(e) => (e.currentTarget.style.backgroundColor = "transparent")}
                  >
                    Remove
                  </button>
                </div>
              ))}
            </div>
          </div>
        )}

        {/* Add input */}
        <div className="flex gap-2 mb-3">
          <input
            ref={inputRef}
            type="text"
            value={input}
            onChange={(e) => setInput(e.target.value)}
            onKeyDown={handleKeyDown}
            placeholder="e.g. spotify.exe"
            className="flex-1 px-3 py-2 rounded-md text-xs focus:outline-none"
            style={{
              backgroundColor: "var(--bg-tile)",
              border: "1px solid var(--border-subtle)",
              color: "var(--text-primary)",
            }}
          />
          <button
            onClick={handleAdd}
            className="px-3 py-2 text-xs font-semibold text-white rounded-md cursor-pointer"
            style={{ backgroundColor: "var(--color-accent)" }}
            onMouseEnter={(e) => (e.currentTarget.style.backgroundColor = "var(--color-accent-hover)")}
            onMouseLeave={(e) => (e.currentTarget.style.backgroundColor = "var(--color-accent)")}
          >
            Add
          </button>
        </div>

        {error && (
          <p className="text-xs mb-2" style={{ color: "#f87171" }}>
            {error}
          </p>
        )}

        {/* Pick from running */}
        <div className="flex-1 min-h-0 flex flex-col">
          <div className="flex items-center justify-between mb-1.5">
            <p className="text-xs font-medium" style={{ color: "var(--text-secondary)" }}>
              Running processes
            </p>
            {loading && (
              <span className="text-xs" style={{ color: "var(--text-secondary)" }}>
                Loading…
              </span>
            )}
          </div>
          <input
            type="text"
            value={filter}
            onChange={(e) => setFilter(e.target.value)}
            placeholder="Filter…"
            className="w-full px-3 py-1.5 rounded-md text-xs focus:outline-none mb-1.5"
            style={{
              backgroundColor: "var(--bg-tile)",
              border: "1px solid var(--border-subtle)",
              color: "var(--text-primary)",
            }}
          />
          <div
            className="flex-1 overflow-y-auto rounded-md"
            style={{
              maxHeight: 200,
              border: "1px solid var(--border-subtle)",
              backgroundColor: "var(--bg-tile)",
            }}
          >
            {filteredRunning.length === 0 ? (
              <p
                className="text-xs px-3 py-3 text-center"
                style={{ color: "var(--text-secondary)" }}
              >
                {loading ? "Scanning…" : "No matching processes"}
              </p>
            ) : (
              filteredRunning.map((stem) => (
                <button
                  key={stem}
                  onClick={() => handlePickRunning(stem)}
                  className="w-full text-left px-3 py-1.5 text-xs font-mono cursor-pointer transition-colors"
                  style={{
                    color: "var(--text-primary)",
                    borderBottom: "1px solid var(--border-subtle)",
                  }}
                  onMouseEnter={(e) => (e.currentTarget.style.backgroundColor = "var(--bg-tile-hover)")}
                  onMouseLeave={(e) => (e.currentTarget.style.backgroundColor = "transparent")}
                >
                  {stem}
                </button>
              ))
            )}
          </div>
        </div>
      </div>
    </div>
  );
}
