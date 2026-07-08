import { useState, useEffect, useRef } from "react";

interface NamePromptModalProps {
  isOpen: boolean;
  defaultName: string;
  onConfirm: (name: string) => void;
  onCancel: () => void;
}

export function NamePromptModal({
  isOpen,
  defaultName,
  onConfirm,
  onCancel,
}: NamePromptModalProps) {
  const [name, setName] = useState(defaultName);
  const inputRef = useRef<HTMLInputElement>(null);

  // The parent remounts this modal (via key) each time it opens, so `name`
  // re-initialises from defaultName without needing setState in an effect.
  useEffect(() => {
    if (isOpen) {
      const t = setTimeout(() => inputRef.current?.select(), 50);
      return () => clearTimeout(t);
    }
  }, [isOpen]);

  if (!isOpen) return null;

  const handleSubmit = () => {
    onConfirm(name.trim() || defaultName);
  };

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "Enter") handleSubmit();
    if (e.key === "Escape") onCancel();
  };

  return (
    <div
      className="fixed inset-0 z-40 flex items-center justify-center"
      style={{ backgroundColor: "rgba(0,0,0,0.6)" }}
      onClick={onCancel}
    >
      <div
        className="rounded-xl shadow-2xl p-6 w-80"
        style={{ backgroundColor: "var(--bg-card)" }}
        onClick={(e) => e.stopPropagation()}
      >
        <h2
          className="text-sm font-semibold mb-4"
          style={{ color: "var(--text-primary)" }}
        >
          Name your snapshot
        </h2>
        <input
          ref={inputRef}
          type="text"
          value={name}
          onChange={(e) => setName(e.target.value)}
          onKeyDown={handleKeyDown}
          placeholder={defaultName}
          className="w-full px-3 py-2 rounded-md text-sm focus:outline-none mb-4"
          style={{
            backgroundColor: "var(--bg-tile)",
            border: "1px solid var(--border-subtle)",
            color: "var(--text-primary)",
          }}
        />
        <div className="flex justify-end gap-2">
          <button
            onClick={onCancel}
            className="px-4 py-1.5 text-xs rounded-md cursor-pointer transition-colors"
            style={{
              color: "var(--text-secondary)",
              backgroundColor: "transparent",
            }}
            onMouseEnter={(e) =>
              (e.currentTarget.style.backgroundColor = "var(--bg-tile)")
            }
            onMouseLeave={(e) =>
              (e.currentTarget.style.backgroundColor = "transparent")
            }
          >
            Cancel
          </button>
          <button
            onClick={handleSubmit}
            className="px-4 py-1.5 text-xs font-semibold text-white rounded-md cursor-pointer"
            style={{ backgroundColor: "var(--color-accent)" }}
            onMouseEnter={(e) =>
              (e.currentTarget.style.backgroundColor = "var(--color-accent-hover)")
            }
            onMouseLeave={(e) =>
              (e.currentTarget.style.backgroundColor = "var(--color-accent)")
            }
          >
            Save
          </button>
        </div>
      </div>
    </div>
  );
}
