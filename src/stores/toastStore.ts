import { create } from "zustand";

export interface Toast {
  id: number;
  /** One line, human-readable. */
  message: string;
  /** The raw error, shown in the drawer and copied. */
  detail?: string;
  /**
   * Optional URL rendered as a link in the toast body. Opens in the system
   * browser — never the webview. Used by the update-check notice.
   */
  href?: string;
  hrefLabel?: string;
}

/** A burst of failures must not fill the window. */
const MAX_TOASTS = 4;

let nextId = 1;

interface ToastStore {
  toasts: Toast[];
  push: (message: string, detail?: string, link?: { href: string; label?: string }) => number;
  dismiss: (id: number) => void;
  clear: () => void;
}

export const useToastStore = create<ToastStore>((set, get) => ({
  toasts: [],

  push: (message, detail, link) => {
    const id = nextId++;
    const toast: Toast = {
      id,
      message,
      ...(detail !== undefined ? { detail } : {}),
      ...(link ? { href: link.href, hrefLabel: link.label } : {}),
    };
    const toasts = [...get().toasts, toast];
    // Cap by dropping the oldest so a storm cannot stack off-screen.
    set({ toasts: toasts.length > MAX_TOASTS ? toasts.slice(-MAX_TOASTS) : toasts });
    return id;
  },

  dismiss: (id) => set({ toasts: get().toasts.filter((toast) => toast.id !== id) }),

  clear: () => set({ toasts: [] }),
}));
