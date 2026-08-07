import { useState } from "react";
import { useToastStore, type Toast } from "../stores/toastStore";

/**
 * Non-modal error stack. Lives above the status bar, below modal overlays
 * (`z-40` vs their `z-50`), and never traps focus or blocks interaction.
 */
export default function Toasts() {
  const toasts = useToastStore((s) => s.toasts);
  if (toasts.length === 0) return null;

  return (
    <div
      className="pointer-events-none fixed bottom-8 right-3 z-40 flex w-80 max-w-[calc(100vw-1.5rem)] flex-col gap-2"
      aria-live="polite"
    >
      {toasts.map((toast) => (
        <ToastCard key={toast.id} toast={toast} />
      ))}
    </div>
  );
}

function ToastCard({ toast }: { toast: Toast }) {
  const dismiss = useToastStore((s) => s.dismiss);
  const [open, setOpen] = useState(false);
  const [copied, setCopied] = useState(false);

  const copy = async () => {
    if (!toast.detail) return;
    try {
      await navigator.clipboard.writeText(toast.detail);
      setCopied(true);
      window.setTimeout(() => setCopied(false), 1500);
    } catch {
      // Clipboard denied: leave the button alone rather than toasting about a toast.
    }
  };

  const openLink = () => {
    if (!toast.href) return;
    void import("@tauri-apps/plugin-opener")
      .then(({ openUrl }) => openUrl(toast.href!))
      .catch(() => {
        // A dead link must not throw.
      });
  };

  return (
    <div className="pointer-events-auto rounded border border-border bg-danger/10 px-2 py-1.5 shadow-sm">
      <div className="flex items-start gap-2">
        <div className="min-w-0 flex-1 text-xs text-danger">
          <span className="break-words">{toast.message}</span>
          {toast.href && (
            <>
              {" "}
              <button type="button" onClick={openLink} className="underline hover:text-text">
                {toast.hrefLabel ?? "View release"}
              </button>
            </>
          )}
        </div>
        <button
          type="button"
          onClick={() => dismiss(toast.id)}
          aria-label="Dismiss"
          className="shrink-0 text-xs text-text-muted hover:text-text"
        >
          ✕
        </button>
      </div>

      {toast.detail && (
        <div className="mt-1">
          <button
            type="button"
            onClick={() => setOpen((v) => !v)}
            className="text-[11px] text-text-muted underline hover:text-text"
          >
            {open ? "Hide details" : "Details"}
          </button>
          {open && (
            <div className="mt-1">
              <pre className="max-h-32 overflow-auto whitespace-pre-wrap break-words font-mono text-xs text-text-body">
                {toast.detail}
              </pre>
              <button
                type="button"
                onClick={() => void copy()}
                className="mt-1 text-[11px] text-text-muted underline hover:text-text"
              >
                {copied ? "Copied" : "Copy"}
              </button>
            </div>
          )}
        </div>
      )}
    </div>
  );
}
