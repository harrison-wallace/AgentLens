/**
 * Markdown rendering for the preview pane.
 *
 * The markdown comes from whatever repository is being watched, so it is
 * untrusted input: raw markdown may contain HTML, and this runs inside a
 * webview with an IPC bridge to the backend. Every rendered document is
 * therefore sanitized before it reaches the DOM, and navigation is blocked
 * separately by the component (see `Preview.tsx`) — a sanitized `href` is
 * still a link that would take the whole app off-page.
 *
 * Loaded dynamically so `marked` and DOMPurify stay out of the initial
 * bundle; only opening a markdown file pays for them.
 */

/** Sanitized HTML for `source`, or `null` if rendering failed. */
export async function renderMarkdown(source: string): Promise<string | null> {
  try {
    const [{ marked }, { default: DOMPurify }] = await Promise.all([
      import("marked"),
      import("dompurify"),
    ]);

    const html = await marked.parse(source, { async: true, gfm: true, breaks: false });
    return DOMPurify.sanitize(html, {
      // No <script>, no event handlers, and no embedded frames or objects
      // that could reach the network or the IPC bridge.
      FORBID_TAGS: ["script", "style", "iframe", "object", "embed", "form", "input", "button"],
      FORBID_ATTR: ["style", "srcset", "formaction", "ping"],
      // Data URIs are how local images would arrive, but markdown can't
      // reference workspace files safely, so only plain schemes are allowed.
      ALLOWED_URI_REGEXP: /^(?:https?:|mailto:|#)/i,
    });
  } catch {
    return null;
  }
}
