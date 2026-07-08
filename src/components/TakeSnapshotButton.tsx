interface TakeSnapshotButtonProps {
  variant: "centered" | "header";
  onClick: () => void;
}

export function TakeSnapshotButton({ variant, onClick }: TakeSnapshotButtonProps) {
  const base =
    "cursor-pointer font-semibold text-white rounded-md transition-all active:scale-95 select-none";

  if (variant === "centered") {
    return (
      <button
        onClick={onClick}
        className={`${base} px-7 py-3 text-base`}
        style={{ backgroundColor: "var(--color-accent)" }}
        onMouseEnter={(e) =>
          (e.currentTarget.style.backgroundColor = "var(--color-accent-hover)")
        }
        onMouseLeave={(e) =>
          (e.currentTarget.style.backgroundColor = "var(--color-accent)")
        }
      >
        Take Snapshot
      </button>
    );
  }

  return (
    <button
      onClick={onClick}
      className={`${base} px-6 py-2.5 text-sm`}
      style={{ backgroundColor: "var(--color-accent)" }}
      onMouseEnter={(e) =>
        (e.currentTarget.style.backgroundColor = "var(--color-accent-hover)")
      }
      onMouseLeave={(e) =>
        (e.currentTarget.style.backgroundColor = "var(--color-accent)")
      }
    >
      Take Snapshot
    </button>
  );
}
