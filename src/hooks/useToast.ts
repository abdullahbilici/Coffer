import { useCallback, useEffect, useRef, useState } from "react";

const VISIBLE_MS = { success: 1600, error: 4000 };
/** Matches the exit animation in Toast.module.css. */
const EXIT_MS = 180;

export type ToastTone = "success" | "error";

export type ToastState = {
  id: number;
  message: string;
  tone: ToastTone;
  leaving: boolean;
};

export function useToast() {
  const [toast, setToast] = useState<ToastState | null>(null);
  const nextId = useRef(0);

  useEffect(() => {
    if (!toast) return;
    if (toast.leaving) {
      const timer = setTimeout(() => setToast(null), EXIT_MS);
      return () => clearTimeout(timer);
    }
    const timer = setTimeout(
      () => setToast((t) => (t ? { ...t, leaving: true } : t)),
      VISIBLE_MS[toast.tone],
    );
    return () => clearTimeout(timer);
  }, [toast]);

  const show = useCallback((message: string, tone: ToastTone = "error") => {
    setToast({ id: nextId.current++, message, tone, leaving: false });
  }, []);

  const dismiss = useCallback(() => setToast((t) => (t ? { ...t, leaving: true } : t)), []);

  return { toast, show, dismiss };
}
