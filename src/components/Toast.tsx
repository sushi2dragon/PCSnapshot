import { useEffect } from "react";

interface ToastProps {
  message: string | null;
  type: "success" | "warning";
  onDismiss: () => void;
}

export function Toast({ message, type, onDismiss }: ToastProps) {
  useEffect(() => {
    if (!message) return;
    const timer = setTimeout(onDismiss, 4000);
    return () => clearTimeout(timer);
  }, [message, onDismiss]);

  if (!message) return null;

  return (
    <div className="fixed bottom-6 left-1/2 -translate-x-1/2 z-50 animate-[slideUp_0.2s_ease-out]">
      <div
        className={`px-5 py-3 rounded-lg shadow-lg text-white text-sm font-medium ${
          type === "success" ? "bg-emerald-600" : "bg-amber-600"
        }`}
      >
        {message}
      </div>
    </div>
  );
}
