import { useCallback, useEffect, useRef, useState } from "react";

/** Pixels moved per arrow-key press when the splitter has focus. */
const KEYBOARD_STEP = 16;

interface SplitterProps {
  /** Current width of the panel being resized. */
  width: number;
  onResize: (width: number) => void;
  /** Which side the resized panel is on: dragging right grows a left panel
   * and shrinks a right one. */
  side: "left" | "right";
  label: string;
}

/**
 * A one-pixel drag handle between two panels. Pointer capture keeps the drag
 * alive when the cursor outruns the handle, which is otherwise the classic
 * way a splitter "sticks" mid-drag.
 */
export default function Splitter({ width, onResize, side, label }: SplitterProps) {
  const [dragging, setDragging] = useState(false);
  const origin = useRef({ x: 0, width: 0 });

  const onPointerDown = (event: React.PointerEvent<HTMLDivElement>) => {
    event.currentTarget.setPointerCapture(event.pointerId);
    origin.current = { x: event.clientX, width };
    setDragging(true);
  };

  const onPointerMove = (event: React.PointerEvent<HTMLDivElement>) => {
    if (!dragging) return;
    const delta = event.clientX - origin.current.x;
    onResize(origin.current.width + (side === "left" ? delta : -delta));
  };

  const stop = useCallback((event: React.PointerEvent<HTMLDivElement>) => {
    event.currentTarget.releasePointerCapture(event.pointerId);
    setDragging(false);
  }, []);

  // A drag that ends outside the window never fires pointerup on the handle,
  // so the cursor override would otherwise stay stuck on.
  useEffect(() => {
    if (!dragging) return;
    document.body.style.cursor = "col-resize";
    document.body.style.userSelect = "none";
    return () => {
      document.body.style.cursor = "";
      document.body.style.userSelect = "";
    };
  }, [dragging]);

  const onKeyDown = (event: React.KeyboardEvent) => {
    const direction = event.key === "ArrowLeft" ? -1 : event.key === "ArrowRight" ? 1 : 0;
    if (direction === 0) return;
    event.preventDefault();
    onResize(width + direction * KEYBOARD_STEP * (side === "left" ? 1 : -1));
  };

  return (
    <div
      role="separator"
      aria-orientation="vertical"
      aria-label={label}
      tabIndex={0}
      onPointerDown={onPointerDown}
      onPointerMove={onPointerMove}
      onPointerUp={stop}
      onPointerCancel={stop}
      onKeyDown={onKeyDown}
      className={`w-1 shrink-0 cursor-col-resize bg-border transition-colors hover:bg-glow focus:bg-glow focus:outline-none ${
        dragging ? "bg-glow" : ""
      }`}
    />
  );
}
