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
 * Fenced `mermaid` blocks render to SVG through mermaid itself, then that
 * SVG is sanitized on a separate path that allows the style mermaid needs.
 * Mermaid's `%%{init}%%` and YAML frontmatter can flip `securityLevel` to
 * `loose`, so those directives are stripped before render. The mermaid
 * bundle loads only when a fence is present.
 *
 * Loaded dynamically so `marked`, DOMPurify, and mermaid stay out of the
 * initial bundle; only opening a markdown file pays for them.
 */

import type { Config as DomPurifyConfig, DOMPurify } from "dompurify";

/** Cap so a hostile file cannot queue dozens of mermaid layouts. */
export const MAX_MERMAID_DIAGRAMS = 16;

export type MarkdownTheme = "dark" | "light";

/** Injected in tests so node does not have to boot mermaid's DOM renderer. */
export type MermaidSvgRenderer = (id: string, source: string) => Promise<string>;

export interface RenderMarkdownOptions {
  theme?: MarkdownTheme;
  renderMermaid?: MermaidSvgRenderer;
}

const MARKDOWN_PURIFY: DomPurifyConfig = {
  // No <script>, no event handlers, and no embedded frames or objects
  // that could reach the network or the IPC bridge.
  FORBID_TAGS: ["script", "style", "iframe", "object", "embed", "form", "input", "button"],
  FORBID_ATTR: ["style", "srcset", "formaction", "ping"],
  // Data URIs are how local images would arrive, but markdown can't
  // reference workspace files safely, so only plain schemes are allowed.
  ALLOWED_URI_REGEXP: /^(?:https?:|mailto:|#)/i,
};

/**
 * Mermaid SVGs need `<style>` and inline style; the markdown profile
 * forbids both. This profile still drops script, frames, and HTML
 * foreignObject (the usual mermaid XSS vehicles).
 *
 * Do not set `ALLOWED_URI_REGEXP` here. DOMPurify applies that check to
 * every attribute that is not on its inert list, so the markdown-link
 * pattern (`https?` / `mailto` / `#`) would also drop `viewBox`, path
 * `d`, `width`, and `marker-end="url(#…)"` — the geometry the diagram
 * needs. The default regexp already rejects `javascript:` while leaving
 * those values alone.
 */
const MERMAID_PURIFY: DomPurifyConfig = {
  USE_PROFILES: { svg: true, svgFilters: true },
  FORBID_TAGS: ["script", "iframe", "object", "embed", "foreignObject", "form", "input", "button"],
  FORBID_ATTR: ["srcset", "formaction", "ping"],
};

const SLOT_ATTR = "data-al-mermaid";
const SLOT_INDEX_ATTR = "data-al-i";

let mermaidSeq = 0;

/** Sanitized HTML for `source`, or `null` if rendering failed. */
export async function renderMarkdown(
  source: string,
  options: RenderMarkdownOptions = {},
): Promise<string | null> {
  try {
    const [{ marked, Renderer }, purifyMod] = await Promise.all([
      import("marked"),
      import("dompurify"),
    ]);
    const DOMPurify = bindPurify(purifyMod.default);

    const diagrams: string[] = [];
    const nonce = newSlotNonce();
    const renderer = new Renderer();
    const defaultCode = renderer.code.bind(renderer);
    renderer.code = (token) => {
      if (token.lang === "mermaid") {
        const index = diagrams.length;
        diagrams.push(token.text);
        return `<div ${SLOT_ATTR}="${nonce}" ${SLOT_INDEX_ATTR}="${index}"></div>`;
      }
      return defaultCode(token);
    };

    const parsed = await marked.parse(source, {
      async: true,
      gfm: true,
      breaks: false,
      renderer,
    });
    const html = String(DOMPurify.sanitize(parsed, MARKDOWN_PURIFY));
    if (diagrams.length === 0) return html;

    const svgs = await renderMermaidBlocks(diagrams, options, DOMPurify);
    const slot = slotPattern(nonce);
    return html.replace(slot, (_match, index) => svgs[Number(index)] ?? mermaidFallback(""));
  } catch {
    return null;
  }
}

async function renderMermaidBlocks(
  diagrams: string[],
  options: RenderMarkdownOptions,
  DOMPurify: DOMPurify,
): Promise<string[]> {
  const renderOne = options.renderMermaid ?? defaultMermaidRenderer(options.theme ?? "dark");
  const out: string[] = [];
  for (let i = 0; i < diagrams.length; i++) {
    const source = diagrams[i] ?? "";
    if (i >= MAX_MERMAID_DIAGRAMS) {
      out.push(mermaidFallback(source));
      continue;
    }
    try {
      mermaidSeq += 1;
      const svg = await renderOne(`al-mmd-${mermaidSeq}`, lockMermaidSource(source));
      const clean = sanitizeMermaidSvg(DOMPurify, svg);
      out.push(clean ? `<div class="mermaid-diagram">${clean}</div>` : mermaidFallback(source));
    } catch {
      out.push(mermaidFallback(source));
    }
  }
  return out;
}

function defaultMermaidRenderer(theme: MarkdownTheme): MermaidSvgRenderer {
  let mermaidReady: Promise<typeof import("mermaid").default> | null = null;
  return async (id, source) => {
    mermaidReady ??= import("mermaid").then(({ default: mermaid }) => {
      mermaid.initialize({
        startOnLoad: false,
        securityLevel: "strict",
        theme: theme === "light" ? "default" : "dark",
        fontFamily: "ui-sans-serif, system-ui, sans-serif",
        flowchart: { htmlLabels: false, useMaxWidth: true },
        sequence: { useMaxWidth: true },
        suppressErrorRendering: true,
      });
      return mermaid;
    });
    const { svg } = await (await mermaidReady).render(id, source);
    return svg;
  };
}

/**
 * Drop the two mermaid config channels that can raise `securityLevel` or
 * turn HTML labels back on. `initialize({ securityLevel: "strict" })` is
 * not enough on its own — a diagram's `%%{init}%%` overrides it.
 */
export function lockMermaidSource(source: string): string {
  return source
    .replace(/^---\r?\n[\s\S]*?\r?\n---(?:\r?\n|$)/, "")
    .replace(/%%\{[\s\S]*?\}%%/g, "");
}

function isPurifyInstance(value: unknown): value is DOMPurify {
  return typeof (value as { sanitize?: unknown } | null)?.sanitize === "function";
}

function bindPurify(exported: unknown): DOMPurify {
  if (isPurifyInstance(exported)) return exported;
  if (typeof exported === "function" && typeof globalThis.window !== "undefined") {
    const bound = (exported as (root: Window) => unknown)(globalThis.window);
    if (isPurifyInstance(bound)) return bound;
  }
  throw new Error("DOMPurify requires a window");
}

function sanitizeMermaidSvg(purify: DOMPurify, svg: string): string {
  purify.addHook("uponSanitizeElement", stripHostileStyle);
  try {
    return String(purify.sanitize(svg, MERMAID_PURIFY));
  } finally {
    purify.removeHook("uponSanitizeElement");
  }
}

function stripHostileStyle(this: DOMPurify, node: Node): void {
  if (node.nodeName !== "STYLE") return;
  const css = node.textContent ?? "";
  if (/@import|expression\s*\(|url\s*\(\s*['"]?\s*javascript:/i.test(css)) {
    node.parentNode?.removeChild(node);
  }
}

function mermaidFallback(source: string): string {
  return `<pre class="mermaid-fallback"><code>${escapeHtml(source)}</code></pre>`;
}

function escapeHtml(text: string): string {
  return text
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;");
}

function newSlotNonce(): string {
  return globalThis.crypto?.randomUUID?.() ?? `n${Date.now()}`;
}

function slotPattern(nonce: string): RegExp {
  return new RegExp(`<div ${SLOT_ATTR}="${nonce}" ${SLOT_INDEX_ATTR}="(\\d+)"></div>`, "g");
}
