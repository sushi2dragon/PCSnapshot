import { useEffect, useRef, useState } from "react";

export type KebabItem = { label: string; onClick: () => void; danger?: boolean };

/** A small three-dots (⋮) menu. Closes on outside click or Escape. */
export function KebabMenu({ items, label = "More actions" }: { items: KebabItem[]; label?: string }) {
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    const onDoc = (e: MouseEvent) => {
      if (ref.current && !ref.current.contains(e.target as Node)) setOpen(false);
    };
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && setOpen(false);
    document.addEventListener("mousedown", onDoc);
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("mousedown", onDoc);
      document.removeEventListener("keydown", onKey);
    };
  }, [open]);

  return (
    <div className="kebab" ref={ref}>
      <button
        type="button"
        className="kebab-button"
        aria-label={label}
        aria-haspopup="menu"
        aria-expanded={open}
        onClick={(e) => {
          e.stopPropagation();
          setOpen((o) => !o);
        }}
      >
        ⋮
      </button>
      {open && (
        <div className="kebab-menu" role="menu">
          {items.map((it, i) => (
            <button
              key={i}
              type="button"
              role="menuitem"
              className={it.danger ? "danger" : ""}
              onClick={(e) => {
                e.stopPropagation();
                setOpen(false);
                it.onClick();
              }}
            >
              {it.label}
            </button>
          ))}
        </div>
      )}
    </div>
  );
}
