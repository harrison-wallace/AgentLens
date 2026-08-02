import { useEffect, useRef } from "react";
import { usePreviewStore, type OpenTab } from "../stores/previewStore";

function basename(path: string): string {
  const cut = path.lastIndexOf("/");
  return cut === -1 ? path : path.slice(cut + 1);
}

/**
 * VS Code-style file tab strip for the preview pane.
 *
 * Permanent tabs stay until closed; a single non-permanent "preview" tab is
 * italic and replaced by the next single-click. Overflow scrolls horizontally;
 * the active tab is scrolled into view.
 */
export default function PreviewTabs() {
  const tabs = usePreviewStore((s) => s.tabs);
  const activePath = usePreviewStore((s) => s.activePath);
  const activate = usePreviewStore((s) => s.activate);
  const close = usePreviewStore((s) => s.close);
  const scrollerRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!activePath || !scrollerRef.current) return;
    // Match by data attribute without CSS.escape (paths can contain quotes).
    for (const child of scrollerRef.current.children) {
      if (!(child instanceof HTMLElement)) continue;
      if (child.dataset.tabPath === activePath) {
        child.scrollIntoView({ block: "nearest", inline: "nearest" });
        break;
      }
    }
  }, [activePath, tabs]);

  if (tabs.length === 0) return null;

  return (
    <div
      ref={scrollerRef}
      role="tablist"
      aria-label="Open files"
      className="flex h-7 shrink-0 items-stretch gap-0 overflow-x-auto border-b border-border [scrollbar-width:thin]"
    >
      {tabs.map((tab) => (
        <FileTab
          key={tab.path}
          tab={tab}
          active={tab.path === activePath}
          onActivate={() => void activate(tab.path)}
          onClose={() => void close(tab.path)}
        />
      ))}
    </div>
  );
}

function FileTab({
  tab,
  active,
  onActivate,
  onClose,
}: {
  tab: OpenTab;
  active: boolean;
  onActivate: () => void;
  onClose: () => void;
}) {
  return (
    <div
      data-tab-path={tab.path}
      role="tab"
      aria-selected={active}
      title={tab.path}
      onClick={onActivate}
      onMouseDown={(event) => {
        // Middle-click close (button 1); don't steal focus with preventDefault
        // on the whole strip for left clicks.
        if (event.button === 1) {
          event.preventDefault();
          onClose();
        }
      }}
      className={`group flex max-w-[12rem] shrink-0 cursor-default items-center gap-1 border-r border-border px-2 text-[11px] ${
        active ? "bg-selected text-text" : "text-text-muted hover:bg-hover hover:text-text"
      } ${tab.permanent ? "" : "italic"}`}
    >
      <span className="min-w-0 flex-1 truncate">{basename(tab.path)}</span>
      <button
        type="button"
        aria-label={`Close ${basename(tab.path)}`}
        title="Close"
        onClick={(event) => {
          event.stopPropagation();
          onClose();
        }}
        className={`flex h-4 w-4 shrink-0 items-center justify-center rounded text-[10px] leading-none ${
          active
            ? "text-text-muted hover:bg-hover hover:text-text"
            : "text-transparent group-hover:text-text-muted hover:bg-hover hover:text-text"
        }`}
      >
        ×
      </button>
    </div>
  );
}
