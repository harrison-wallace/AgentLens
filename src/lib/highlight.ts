/**
 * Shiki wrapper.
 *
 * Grammars are imported by explicit name rather than through Shiki's full
 * bundle: the bundle's dynamic-import map references every language it knows,
 * so the build emits a chunk per grammar (hundreds of files, tens of MB in the
 * installer) even though only a handful are reachable. Listing them here means
 * one chunk per language we actually offer, each fetched on first use.
 *
 * The JavaScript regex engine is used for the same reason — the oniguruma
 * engine drags in a ~600 KB wasm blob. It covers these grammars; anything it
 * can't handle falls back to plain text via the catch below.
 */
import type { HighlighterCore } from "shiki/core";

/**
 * Both ramps are loaded and emitted at once as CSS custom properties, so
 * switching theme is a selector in `index.css` rather than re-highlighting
 * every open file — Shiki's cost is in the tokenizing, not the styling.
 */
const THEMES = { light: "github-light-default", dark: "github-dark-default" } as const;

/**
 * Languages available to the preview, mapped to their grammar chunk. A file
 * whose language isn't here renders unhighlighted rather than pulling an
 * arbitrary grammar at runtime.
 */
const LANGUAGE_LOADERS = {
  c: () => import("@shikijs/langs/c"),
  cpp: () => import("@shikijs/langs/cpp"),
  csharp: () => import("@shikijs/langs/csharp"),
  css: () => import("@shikijs/langs/css"),
  docker: () => import("@shikijs/langs/docker"),
  go: () => import("@shikijs/langs/go"),
  html: () => import("@shikijs/langs/html"),
  java: () => import("@shikijs/langs/java"),
  javascript: () => import("@shikijs/langs/javascript"),
  json: () => import("@shikijs/langs/json"),
  jsonc: () => import("@shikijs/langs/jsonc"),
  jsx: () => import("@shikijs/langs/jsx"),
  kotlin: () => import("@shikijs/langs/kotlin"),
  lua: () => import("@shikijs/langs/lua"),
  make: () => import("@shikijs/langs/make"),
  markdown: () => import("@shikijs/langs/markdown"),
  php: () => import("@shikijs/langs/php"),
  powershell: () => import("@shikijs/langs/powershell"),
  python: () => import("@shikijs/langs/python"),
  ruby: () => import("@shikijs/langs/ruby"),
  rust: () => import("@shikijs/langs/rust"),
  scss: () => import("@shikijs/langs/scss"),
  shell: () => import("@shikijs/langs/shellscript"),
  sql: () => import("@shikijs/langs/sql"),
  swift: () => import("@shikijs/langs/swift"),
  toml: () => import("@shikijs/langs/toml"),
  tsx: () => import("@shikijs/langs/tsx"),
  typescript: () => import("@shikijs/langs/typescript"),
  xml: () => import("@shikijs/langs/xml"),
  yaml: () => import("@shikijs/langs/yaml"),
} as const;

type SupportedLanguage = keyof typeof LANGUAGE_LOADERS;

let highlighterPromise: Promise<HighlighterCore> | null = null;
const loaded = new Set<string>();

function isSupported(language: string): language is SupportedLanguage {
  return Object.hasOwn(LANGUAGE_LOADERS, language);
}

/** Created once and reused; grammars are added to it as files are opened. */
async function getHighlighter(): Promise<HighlighterCore> {
  if (!highlighterPromise) {
    highlighterPromise = (async () => {
      const [{ createHighlighterCore }, { createJavaScriptRegexEngine }, dark, light] =
        await Promise.all([
          import("shiki/core"),
          import("shiki/engine/javascript"),
          import("@shikijs/themes/github-dark-default"),
          import("@shikijs/themes/github-light-default"),
        ]);
      return createHighlighterCore({
        themes: [dark.default, light.default],
        langs: [],
        // Skip regex constructs the JS engine can't express instead of
        // throwing; worst case a few tokens go uncoloured.
        engine: createJavaScriptRegexEngine({ forgiving: true }),
      });
    })();
  }
  return highlighterPromise;
}

/**
 * Highlighted HTML for `code`, or `null` if the language isn't supported or
 * Shiki failed to load — callers render plain text in that case rather than
 * showing nothing.
 */
export async function highlightToHtml(code: string, language: string): Promise<string | null> {
  if (!isSupported(language)) return null;
  try {
    const highlighter = await getHighlighter();
    if (!loaded.has(language)) {
      const grammar = await LANGUAGE_LOADERS[language]();
      await highlighter.loadLanguage(grammar.default);
      loaded.add(language);
    }
    // `defaultColor: false` emits `--shiki-light` / `--shiki-dark` instead of
    // baking one ramp into `color:`; index.css picks the side.
    return highlighter.codeToHtml(code, {
      lang: language,
      themes: THEMES,
      defaultColor: false,
    });
  } catch {
    return null;
  }
}
