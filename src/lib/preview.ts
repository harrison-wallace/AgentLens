/**
 * Pure helpers for the preview pane's rendering decisions. Kept out of the
 * component so the thresholds are testable and stated in one place.
 */

/**
 * Above this, a file is shown as plain text instead of highlighted or
 * rendered markdown.
 *
 * The backend's preview cap is 2 MB, which is fine to *display* — React puts
 * plain text in a single text node. Rich rendering is a different cost: Shiki
 * tokenizes into one span per token, so a large but perfectly ordinary file
 * (a ~1 MB `package-lock.json`, say) becomes hundreds of thousands of DOM
 * nodes and blocks the main thread while it's built.
 */
export const RICH_RENDER_MAX_BYTES = 512 * 1024;

/** True if `text` is small enough to highlight or render as markdown. */
export function canRenderRich(text: string): boolean {
  return byteLength(text) <= RICH_RENDER_MAX_BYTES;
}

/**
 * UTF-8 byte length. `String.length` counts UTF-16 units, which undercounts
 * the memory a non-ASCII file actually costs to render.
 */
function byteLength(text: string): number {
  let bytes = 0;
  for (let i = 0; i < text.length; i++) {
    const code = text.charCodeAt(i);
    if (code < 0x80) bytes += 1;
    else if (code < 0x800) bytes += 2;
    else if (code >= 0xd800 && code <= 0xdbff) {
      // Surrogate pair: 4 bytes across two units, counted on the lead.
      bytes += 4;
      i += 1;
    } else bytes += 3;
  }
  return bytes;
}

/** Byte count for display, e.g. "1.4 MB". */
export function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}
