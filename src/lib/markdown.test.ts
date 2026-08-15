import { JSDOM } from "jsdom";
import { describe, expect, it, vi } from "vitest";
import {
  lockMermaidSource,
  MAX_MERMAID_DIAGRAMS,
  renderMarkdown,
  type MermaidSvgRenderer,
} from "./markdown";

// DOMPurify needs a window. The suite runs in node, so give it one here.
const dom = new JSDOM("<!doctype html><html><body></body></html>");
vi.stubGlobal("window", dom.window);
vi.stubGlobal("document", dom.window.document);

const svgOk: MermaidSvgRenderer = async () => "<svg><text>ok</text></svg>";

describe("renderMarkdown", () => {
  it("renders GFM and strips script tags", async () => {
    const html = await renderMarkdown("# Title\n\n<script>alert(1)</script>\n\nHi");
    expect(html).toContain("<h1>");
    expect(html).toContain("Title");
    expect(html).toContain("Hi");
    expect(html).not.toContain("<script>");
    expect(html).not.toContain("alert(1)");
  });

  it("leaves a javascript fence as a code block", async () => {
    const html = await renderMarkdown("```javascript\nconst x = 1;\n```", {
      renderMermaid: async () => {
        throw new Error("mermaid should not run");
      },
    });
    expect(html).toContain("<code");
    expect(html).toContain("const x = 1;");
    expect(html).not.toContain("mermaid-diagram");
  });

  it("replaces a mermaid fence with the sanitized SVG", async () => {
    const html = await renderMarkdown("intro\n\n```mermaid\ngraph TD\nA-->B\n```\n\noutro", {
      renderMermaid: svgOk,
    });
    expect(html).toContain('class="mermaid-diagram"');
    expect(html).toContain("<svg>");
    expect(html).toContain("ok");
    expect(html).toContain("intro");
    expect(html).toContain("outro");
    expect(html).not.toContain("```");
    expect(html).not.toContain("graph TD");
  });

  it("accepts a tilde mermaid fence", async () => {
    const html = await renderMarkdown("~~~\nmermaid\n~~~\n", { renderMermaid: svgOk });
    // A fence whose info string is missing is not mermaid.
    expect(html).not.toContain("mermaid-diagram");

    const withLang = await renderMarkdown("~~~mermaid\ngraph TD\nA-->B\n~~~", {
      renderMermaid: svgOk,
    });
    expect(withLang).toContain("mermaid-diagram");
  });

  it("strips script tags from mermaid SVG output", async () => {
    const html = await renderMarkdown("```mermaid\ngraph TD\nA-->B\n```", {
      renderMermaid: async () => `<svg><script>alert(1)</script><text>safe</text></svg>`,
    });
    expect(html).toContain("<svg>");
    expect(html).toContain("safe");
    expect(html).not.toContain("<script>");
    expect(html).not.toContain("alert(1)");
  });

  it("strips foreignObject from mermaid SVG output", async () => {
    const html = await renderMarkdown("```mermaid\ngraph TD\nA-->B\n```", {
      renderMermaid: async () =>
        `<svg><foreignObject><body><img src=x onerror=alert(1)></body></foreignObject></svg>`,
    });
    expect(html).not.toContain("foreignObject");
    expect(html).not.toContain("onerror");
  });

  it("falls back to a code block when mermaid throws", async () => {
    const html = await renderMarkdown("```mermaid\nnot a diagram\n```", {
      renderMermaid: async () => {
        throw new Error("parse failed");
      },
    });
    expect(html).toContain("mermaid-fallback");
    expect(html).toContain("not a diagram");
    expect(html).not.toContain("<svg>");
  });

  it("passes the locked source to mermaid, not the raw fence", async () => {
    const seen: string[] = [];
    await renderMarkdown(
      "```mermaid\n%%{init: { 'securityLevel': 'loose' }}%%\ngraph TD\nA-->B\n```",
      {
        renderMermaid: async (_id, source) => {
          seen.push(source);
          return "<svg></svg>";
        },
      },
    );
    expect(seen).toHaveLength(1);
    expect(seen[0]).not.toMatch(/securityLevel|init/);
    expect(seen[0]).toContain("graph TD");
  });

  it("caps the number of mermaid renders", async () => {
    const render = vi.fn(svgOk);
    const fences = Array.from(
      { length: MAX_MERMAID_DIAGRAMS + 3 },
      (_, i) => `\`\`\`mermaid\ngraph TD\nN${i}\n\`\`\``,
    ).join("\n\n");
    const html = await renderMarkdown(fences, { renderMermaid: render });
    expect(render).toHaveBeenCalledTimes(MAX_MERMAID_DIAGRAMS);
    expect(html?.match(/mermaid-fallback/g)?.length).toBe(3);
  });
});

describe("lockMermaidSource", () => {
  it("strips init directives and YAML frontmatter", () => {
    const locked = lockMermaidSource(
      "---\nconfig:\n  securityLevel: loose\n---\n%%{init: { htmlLabels: true }}%%\ngraph TD\nA-->B\n",
    );
    expect(locked).not.toMatch(/securityLevel|htmlLabels|init|config:/);
    expect(locked).toContain("graph TD");
  });
});
