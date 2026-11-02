import { useCallback, useMemo, useState } from "react";

export interface Toast {
  id: number;
  kind: "info" | "error";
  text: string;
}

export interface Toasts {
  list: Toast[];
  info: (t: string) => void;
  error: (t: string) => void;
  dismiss: (id: number) => void;
}

let next = 1;

export function useToasts(): Toasts {
  const [list, setList] = useState<Toast[]>([]);
  const dismiss = useCallback((id: number) => setList((l) => l.filter((t) => t.id !== id)), []);
  const push = useCallback(
    (kind: Toast["kind"], text: string) => {
      const id = next++;
      setList((l) => [...l.slice(-3), { id, kind, text }]);
      window.setTimeout(() => dismiss(id), kind === "error" ? 7000 : 3000);
    },
    [dismiss],
  );
  return useMemo(
    () => ({ list, dismiss, info: (t: string) => push("info", t), error: (t: string) => push("error", t) }),
    [list, dismiss, push],
  );
}

export function ToastHost({ toasts }: { toasts: Toasts }) {
  return (
    <div className="toasts" role="status" aria-live="polite">
      {toasts.list.map((t) => (
        <div key={t.id} className={`toast ${t.kind}`}>
          <span>{t.text}</span>
          <button className="icon-btn" aria-label="Dismiss" onClick={() => toasts.dismiss(t.id)}>
            ×
          </button>
        </div>
      ))}
    </div>
  );
}
