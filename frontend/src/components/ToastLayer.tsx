import { useEffect } from "react";
import { useToastActions } from "../context/AppContext";
import type { ToastItem } from "../types";

function ToastRow({ toast }: { toast: ToastItem }) {
  const actions = useToastActions();
  useEffect(() => {
    const timer = window.setTimeout(() => actions.remove(toast.id), 3200);
    return () => window.clearTimeout(timer);
  }, [actions, toast.id]);

  return (
    <div className={`toast ${toast.type}`}>
      <span>{toast.message}</span>
    </div>
  );
}

export function ToastLayer({ toasts }: { toasts: ToastItem[] }) {
  return (
    <div id="toasts">
      {toasts.map((toast) => <ToastRow key={toast.id} toast={toast} />)}
    </div>
  );
}
