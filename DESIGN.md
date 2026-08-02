---
version: alpha
name: AgentLens-tui
description: |
  A dark, terminal-native observer for agentic coding — not a marketing page,
  not an IDE. The whole product is one dense TUI window: monospaced chrome and
  data, single-letter kind badges, hairline pane splits, and semantic color used
  only for change kinds and state. Practical IDE density (Option B) with a
  manpage vocabulary (mono type, status-bar stats, scarce chrome). No cream
  canvas, no display headlines, no decorative imagery.
  Substitutes for proprietary mono: JetBrains Mono, Geist Mono, or IBM Plex Mono.

colors:
  # Surfaces — dark-only product. Warm near-black, not pure OLED blue-black.
  canvas: "#0f0e0e"
  surface: "#0f0e0e"
  surface-elevated: "#1a1818"
  surface-overlay: "#241f1f"
  hover: "#242020"
  selected: "#2a2626"
  hairline: "rgba(253, 252, 252, 0.08)"
  hairline-strong: "rgba(253, 252, 252, 0.14)"

  # Text ladder
  ink: "#e8e4e4"
  body: "#c8c4c4"
  mute: "#8a8686"
  ash: "#6a6666"
  on-accent: "#0f0e0e"

  # Semantic — scarce; never as large fills outside CTAs
  accent: "#007aff"
  accent-hover: "#0056b3"
  accent-soft: "rgba(0, 122, 255, 0.16)"
  success: "#30d158"
  warning: "#ff9f0a"
  danger: "#ff3b30"
  info: "#64d2ff"

  # Kind badges (git + activity feed share this ramp)
  kind-added: "#30d158"
  kind-modified: "#ff9f0a"
  kind-deleted: "#ff3b30"
  kind-renamed: "#64d2ff"
  kind-untracked: "#bf5af2"
  kind-conflicted: "#ff9f0a"

  # Diff gutters — tinted enough to scan, dim enough to read code over
  diff-added-bg: "rgba(48, 209, 88, 0.12)"
  diff-removed-bg: "rgba(255, 59, 48, 0.12)"

  # Live "just changed" glow (tree)
  glow: "#007aff"

typography:
  # One mono face for chrome + data. Markdown prose may stay proportional
  # (see principles); code fences always mono.
  font-mono: "JetBrains Mono, Geist Mono, IBM Plex Mono, ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace"
  font-prose: "ui-sans-serif, system-ui, -apple-system, Segoe UI, sans-serif"

  display:
    # Empty-state wordmark only — never in the workspace shell
    fontFamily: "{typography.font-mono}"
    fontSize: 22px
    fontWeight: 600
    lineHeight: 1.3
    letterSpacing: 0.02em

  heading:
    fontFamily: "{typography.font-mono}"
    fontSize: 11px
    fontWeight: 700
    lineHeight: 1.4
    letterSpacing: 0.06em
    textTransform: uppercase

  body:
    fontFamily: "{typography.font-mono}"
    fontSize: 12px
    fontWeight: 400
    lineHeight: 1.45
    letterSpacing: 0

  body-strong:
    fontFamily: "{typography.font-mono}"
    fontSize: 12px
    fontWeight: 500
    lineHeight: 1.45
    letterSpacing: 0

  caption:
    fontFamily: "{typography.font-mono}"
    fontSize: 11px
    fontWeight: 400
    lineHeight: 1.4
    letterSpacing: 0

  stat:
    # Status bar + feed toolbar + batch summaries
    fontFamily: "{typography.font-mono}"
    fontSize: 11px
    fontWeight: 400
    lineHeight: 1
    letterSpacing: 0

  code:
    fontFamily: "{typography.font-mono}"
    fontSize: 12px
    fontWeight: 400
    lineHeight: 1.55
    letterSpacing: 0

  prose:
    fontFamily: "{typography.font-prose}"
    fontSize: 13px
    fontWeight: 400
    lineHeight: 1.65
    letterSpacing: 0

rounded:
  none: 0px
  sm: 4px
  full: 9999px

spacing:
  xxs: 1px
  xs: 4px
  sm: 8px
  md: 12px
  lg: 16px
  xl: 24px
  row: 24px
  header: 40px
  status: 28px
  panel-pad-x: 12px

sizing:
  tree-row: 24px
  feed-row: 24px
  header-height: 40px
  status-height: 28px
  toolbar-height: 28px
  button-height: 28px
  input-height: 32px
  panel-min: 180px
  panel-max: 640px
  panel-default: 320px
  touch-min: 28px

components:
  app-shell:
    backgroundColor: "{colors.canvas}"
    textColor: "{colors.ink}"
    typography: "{typography.body}"

  header-bar:
    backgroundColor: "{colors.surface-elevated}"
    textColor: "{colors.ink}"
    borderBottom: "1px solid {colors.hairline}"
    height: "{sizing.header-height}"
    padding: "0 {spacing.panel-pad-x}"
    typography: "{typography.body}"

  status-bar:
    backgroundColor: "{colors.surface-elevated}"
    textColor: "{colors.mute}"
    borderTop: "1px solid {colors.hairline}"
    height: "{sizing.status-height}"
    padding: "0 {spacing.panel-pad-x}"
    typography: "{typography.caption}"

  status-stat:
    typography: "{typography.stat}"
    fontVariantNumeric: tabular-nums
    # Pattern: "M 3" with kind color; gap between stats ~12px

  feed-toolbar:
    backgroundColor: "{colors.canvas}"
    borderBottom: "1px solid {colors.hairline}"
    height: "{sizing.toolbar-height}"
    padding: "0 {spacing.panel-pad-x}"
    typography: "{typography.stat}"

  feed-stat-button:
    backgroundColor: transparent
    typography: "{typography.stat}"
    rounded: "{rounded.sm}"
    padding: "0 2px"
    # Active filter: full kind color + underline; inactive under filter: mute + 50% opacity

  panel-chrome:
    backgroundColor: "{colors.canvas}"
    borderColor: "{colors.hairline}"
    # Panes share one canvas; splits are hairlines, not raised cards

  tree-row:
    height: "{sizing.tree-row}"
    typography: "{typography.body}"
    textColor: "{colors.body}"
    paddingRight: 16px
    # Selected: surface selected; hover: hover; glow: inset accent bar

  tree-row-selected:
    backgroundColor: "{colors.selected}"
    textColor: "{colors.ink}"

  tree-row-hover:
    backgroundColor: "{colors.hover}"

  tree-row-glow:
    # Inset left bar, not a border that shifts layout
    boxShadow: "inset 2px 0 0 {colors.glow}"

  kind-badge:
    typography: "{typography.stat}"
    width: 12px
    textAlign: center
    # Glyphs: + M − → A D ? R ! — never multi-word labels in dense lists

  button-ghost:
    backgroundColor: transparent
    textColor: "{colors.mute}"
    typography: "{typography.caption}"
    rounded: "{rounded.sm}"
    padding: "4px 8px"
    height: "{sizing.button-height}"

  button-ghost-active:
    backgroundColor: transparent
    textColor: "{colors.ink}"

  button-secondary:
    backgroundColor: transparent
    textColor: "{colors.mute}"
    border: "1px solid {colors.hairline-strong}"
    typography: "{typography.caption}"
    rounded: "{rounded.sm}"
    padding: "4px 8px"
    height: "{sizing.button-height}"

  button-primary:
    backgroundColor: transparent
    textColor: "{colors.accent}"
    border: "1px solid {colors.accent}"
    typography: "{typography.caption}"
    fontWeight: 500
    rounded: "{rounded.sm}"
    padding: "4px 8px"
    height: "{sizing.button-height}"

  button-primary-solid:
    # Use sparingly — empty-state CTA, destructive confirm
    backgroundColor: "{colors.ink}"
    textColor: "{colors.on-accent}"
    typography: "{typography.caption}"
    fontWeight: 500
    rounded: "{rounded.sm}"
    padding: "4px 16px"
    height: 32px

  text-input:
    backgroundColor: "{colors.surface-elevated}"
    textColor: "{colors.ink}"
    typography: "{typography.body}"
    rounded: "{rounded.sm}"
    padding: "6px 10px"
    height: "{sizing.input-height}"
    border: "1px solid {colors.hairline}"

  text-input-focused:
    backgroundColor: "{colors.canvas}"
    border: "1px solid {colors.accent}"

  overlay-scrim:
    backgroundColor: "rgba(0, 0, 0, 0.5)"

  modal-panel:
    backgroundColor: "{colors.surface-elevated}"
    textColor: "{colors.ink}"
    border: "1px solid {colors.hairline-strong}"
    rounded: "{rounded.sm}"
    # Prefer hairline + elevation token over heavy drop shadows

  list-row:
    backgroundColor: transparent
    textColor: "{colors.body}"
    typography: "{typography.body}"
    padding: "4px 8px"
    rounded: "{rounded.none}"

  list-row-active:
    backgroundColor: "{colors.selected}"
    textColor: "{colors.ink}"

  section-label:
    typography: "{typography.heading}"
    textColor: "{colors.mute}"
    padding: "6px 8px 2px"

  gap-marker:
    typography: "{typography.caption}"
    textColor: "{colors.mute}"
    # Open gap uses danger text — feed must not silently resume after outage

  gap-marker-open:
    textColor: "{colors.danger}"

  empty-state:
    backgroundColor: "{colors.canvas}"
    textColor: "{colors.mute}"
    typography: "{typography.body}"
    # Manpage tone: short sentences, mono, one primary CTA

  commit-box:
    backgroundColor: "{colors.surface-elevated}"
    borderTop: "1px solid {colors.hairline}"
    padding: "{spacing.sm}"

  diff-line-added:
    backgroundColor: "{colors.diff-added-bg}"

  diff-line-removed:
    backgroundColor: "{colors.diff-removed-bg}"

  splitter:
    # Invisible hit target; visual is the hairline between panes
    width: 4px
    backgroundColor: transparent

  scrollbar-gutter:
    # Prefer stable gutter on scrollports that host row actions on the right
    scrollbarGutter: stable
---

# AgentLens design system

## Overview

AgentLens is a **lightweight open-source observer for agentic coding** — a
read-only window into what a terminal agent is doing to a directory: live file
tree, activity feed, git status, and previews. It is **not** an editor and
**not** a marketing site.

Visually it is a **dark terminal-native product UI** (Option B: practical IDE
density) that borrows the _vocabulary_ of manpage / TUI systems:

- One **monospace** face for chrome, tree, feed, git, and status
- **Single-letter kind badges** (`M`, `A`, `D`, `+`, `−`, `→`, `?`) — never
  English labels in dense lists
- **Hairline** pane splits on a **single flat canvas** (not stacked raised cards)
- **Semantic color only** for change kinds, connection errors, and scarce accents
- **4px radius** on interactive controls; rows and panes stay sharp

This system deliberately **does not** adopt OpenCode-style cream marketing
chrome, 38px display type, or 96px section rhythm. AgentLens _is_ the dark TUI
surface end-to-end.

**Key characteristics**

- Dark-only product theme (no day/cream mode in scope for alpha)
- Mono-first UI; proportional type allowed only for long markdown prose
- Status-bar stats and feed toolbar are the same dialect: `M 3  A 1  D 2`
- ASCII-adjacent glyphs for structure (`▸`, pin marks) without forcing `[+]`
  brackets into every 320px panel column
- Debounced, scannable density — 24px rows, 11–12px type, tabular stats

---

## Principles

1. **Read-only by default.** Previews never look editable; primary mutators
   (git) are explicit and contained.
2. **Scan, don’t decorate.** Color encodes _kind_ or _state_, not brand paint.
3. **One dialect for counts.** Footer git stats, feed toolbar, and batch
   headers all use mono letter + count.
4. **Flat window, hairline panes.** Prefer one canvas + borders over multi-level
   elevation. Overlays may elevate one step.
5. **Density over drama.** No hero moments inside the workspace shell.
6. **Honest gaps.** Connection outages are marked in the feed; never silent resume.
7. **When in doubt, match the status bar.** If a new control can’t be expressed
   as mute mono caption + optional kind color, redesign it.

---

## Colors

### Surfaces

| Token                       | Value                    | Use                                    |
| --------------------------- | ------------------------ | -------------------------------------- |
| `{colors.canvas}`           | `#0f0e0e`                | App background, panel bodies           |
| `{colors.surface-elevated}` | `#1a1818`                | Header, status bar, modal body, inputs |
| `{colors.surface-overlay}`  | `#241f1f`                | Nested popovers if needed              |
| `{colors.hover}`            | `#242020`                | Row / control hover                    |
| `{colors.selected}`         | `#2a2626`                | Selected tree row, active list row     |
| `{colors.hairline}`         | `rgba(253,252,252,0.08)` | Pane splits, row rules                 |
| `{colors.hairline-strong}`  | `rgba(253,252,252,0.14)` | Modal edge, stronger chrome rules      |

Warm near-black (faint red/brown undertone) preferred over cool blue-black so
the shell doesn’t read as generic “VS Code default dark.”

### Text

| Token           | Value     | Use                                              |
| --------------- | --------- | ------------------------------------------------ |
| `{colors.ink}`  | `#e8e4e4` | Primary labels, selected text, wordmark          |
| `{colors.body}` | `#c8c4c4` | Default tree/feed path text                      |
| `{colors.mute}` | `#8a8686` | Timestamps, roots, status chrome, section labels |
| `{colors.ash}`  | `#6a6666` | Disabled, ignored paths, placeholders            |

### Semantic (scarce)

| Token                                         | Value     | Use                                                            |
| --------------------------------------------- | --------- | -------------------------------------------------------------- |
| `{colors.accent}`                             | `#007aff` | Links, focus, primary outline button, agent-context mark, glow |
| `{colors.success}` / `{colors.kind-added}`    | `#30d158` | Created / added                                                |
| `{colors.warning}` / `{colors.kind-modified}` | `#ff9f0a` | Modified / connecting                                          |
| `{colors.danger}` / `{colors.kind-deleted}`   | `#ff3b30` | Deleted / error / open gap / failed connection                 |
| `{colors.kind-renamed}`                       | `#64d2ff` | Renamed                                                        |
| `{colors.kind-untracked}`                     | `#bf5af2` | Untracked                                                      |

**Do not** use accent as a large panel fill. Prefer outline primary buttons in
chrome; solid ink fills only for empty-state or irreversible confirms.

### Diff

| Token                      | Use                         |
| -------------------------- | --------------------------- |
| `{colors.diff-added-bg}`   | Added lines in session diff |
| `{colors.diff-removed-bg}` | Removed lines               |

Tints stay low so syntax highlighting remains readable.

---

## Typography

### Font stack

- **UI + data + code:** `{typography.font-mono}` — JetBrains Mono (preferred
  open substitute), then Geist Mono, IBM Plex Mono, system mono stack.
- **Markdown prose only (optional):** `{typography.font-prose}` for paragraph
  readability in the preview pane. Fenced code and inline code stay mono.
- **No** second display face. **No** italics as a brand device.

Berkeley Mono is an acceptable commercial substitute where licensed; the open
stack above is the default for an MIT project.

### Scale

| Role                       | Size | Weight | Line height | Where                                       |
| -------------------------- | ---- | ------ | ----------- | ------------------------------------------- |
| `{typography.display}`     | 22px | 600    | 1.3         | Empty-state title only                      |
| `{typography.heading}`     | 11px | 700    | 1.4         | Section labels (`PINNED`, `SOURCE CONTROL`) |
| `{typography.body}`        | 12px | 400    | 1.45        | Tree names, feed paths, dialogs             |
| `{typography.body-strong}` | 12px | 500    | 1.45        | Emphasis, active tabby controls             |
| `{typography.caption}`     | 11px | 400    | 1.4         | Header meta, buttons, status text           |
| `{typography.stat}`        | 11px | 400    | 1           | `M 3`, feed toolbar, batch summaries        |
| `{typography.code}`        | 12px | 400    | 1.55        | Preview / diff                              |
| `{typography.prose}`       | 13px | 400    | 1.65        | Rendered markdown paragraphs                |

Dense app default is **12px mono**, not marketing 16px.

---

## Layout

### Shell

```
┌─ header (40px) ─────────────────────────────────────────────────────────────┐
│ workspace · root · watching since …     [panels] Clear Refresh … Close     │
├─ tree ──┬─ preview ─────────────────────────────┬─ feed ───────────────────┤
│         │                                       │ toolbar (28px)           │
│ 24px    │                                       │ batches / gaps           │
│ rows    │                                       │                          │
│ + git   │                                       │                          │
├─────────┴───────────────────────────────────────┴──────────────────────────┤
│ status (28px): branch  M n  A n  D n  ? n     remote?     watcher: on      │
└────────────────────────────────────────────────────────────────────────────┘
```

- Window never scrolls; **panels own scrolling**.
- Default panel width **320px**; clamp **180–640px**.
- At least one of tree / preview / feed remains visible (layout store rule).
- Splitters are **hairline + hit target**, not thick grips.

### Spacing

Base unit **4px**, with **8px** as the common gap. Row height **24px** for tree
and feed lines. Header **40px**, status and feed toolbar **28px**.

### Radius

| Token                | Use                                       |
| -------------------- | ----------------------------------------- |
| `{rounded.none}`     | Rows, panes, section blocks               |
| `{rounded.sm}` (4px) | Buttons, inputs, modals, icon hit targets |
| `{rounded.full}`     | Rare (e.g. future chips only if needed)   |

---

## Kind badge language

Shared across **activity feed**, **git panel**, **status bar**, and **tree**.

### Glyphs

| Kind            | Feed / FS | Git status | Notes                            |
| --------------- | --------- | ---------- | -------------------------------- |
| Created / added | `+`       | `A`        | Green `{colors.kind-added}`      |
| Modified        | `M`       | `M`        | Amber `{colors.kind-modified}`   |
| Deleted         | `−`       | `D`        | Red `{colors.kind-deleted}`      |
| Renamed         | `→`       | `R`        | Cyan `{colors.kind-renamed}`     |
| Untracked       | —         | `?`        | Purple `{colors.kind-untracked}` |
| Conflicted      | —         | `!`        | Amber `{colors.kind-conflicted}` |

### Compact stats pattern

Status-bar style, always mono, tabular if available:

```text
M 3  A 1  D 2  ? 0
+ 12  M 48  − 5  → 1
```

- Letter, space, count
- ~12px gap between stats
- Kind color on the whole token (`M 3`), not only the letter
- **No** words (`3 modified`) in toolbars or batch headers

### Filters & sort (feed)

- Toolbar totals = **full live feed**, not the active filter (footer-like)
- Click kind = toggle filter; empty selection = show all
- Active kind: full color + underline
- Inactive while filtering: mute + reduced opacity
- Sort control: mono caption cycling `time` → `most +` → `most −` → `most`

---

## Components

### Header bar

- Elevated surface, hairline bottom
- **Left:** workspace name (ink, strong) + root path (mute, truncate)
- **Meta:** `watching since HH:MM` (mute caption)
- **Right:** panel visibility toggles, Clear, Refresh, ignored toggle, settings, Close
- Panel toggles: ghost icon/glyph buttons; inactive = mute + 50% opacity
- Refresh = `{component.button-primary}` (accent outline)
- Clear / Close = `{component.button-secondary}`

### Status bar

- Elevated surface, hairline top, mute caption
- Branch control or plain branch label
- Git stats when `isRepository`: `M n  A n  D n  ? n`
- Remote connection chip **only when remote** (local sessions stay quiet)
- Watcher label: `watcher: on` | `watcher: off` | `watcher: error (…)`

### File tree

- Virtualized 24px rows, depth indent 14px + 8px base
- Chevron `▸` for directories; mono body for names
- Trailing kind/git badge + pin control; **right padding ≥ 16px** so overlay
  scrollbars never cover the pin
- Scrollport: `scrollbar-gutter: stable` when supported
- Ignored: italic + ash/mute
- Agent context: accent `◆` (or keep current mark) with tooltip
- Pin: hollow/filled star or equivalent; unpinned visible on row hover only
- Recently changed: inset glow bar, not layout-shifting border

### Activity feed

- Toolbar + scrollable batches
- Batch header: relative time (mute) + compact kind stats (colored mono)
- Rows: kind badge + path; click reveals in tree
- Cap visible rows per batch; `+N more` in mute caption
- Gap markers: hairline rules + clock range; open gaps use danger color

### Git / source control

- Section label uppercase mute heading
- Staged / changes groups; mono path rows + kind badge
- Commit box on elevated strip; keep actions terminal-honest (hooks run via CLI)

### Preview

- Tabs for current vs diff-since-session (mute / ink active)
- Code: mono, Shiki colors on transparent/surface background
- Diff lines: low-tint added/removed backgrounds
- Markdown: optional proportional prose; code fences mono; accents for links

### Overlays (command palette, settings)

- Scrim 50% black
- Panel: elevated + strong hairline + 4px radius
- Prefer minimal shadow; if shadow is needed for stacking context, keep it soft
  and secondary to the border
- Lists use selected row token; fuzzy match highlight uses accent/glow

### Empty state

- Centered column on canvas
- Mono display wordmark + version caption
- One primary open-folder CTA
- Recent paths as mute list rows
- Remote open flows use the same type scale; no illustrations

---

## Motion

- Prefer **no** ornamental motion
- Allowed: tree glow fade (~3s) into residual tint; relative-time feed tick (~15s)
- No page transitions, no bouncing pins, no gradient shimmers

---

## Accessibility

- Interactive targets ≥ ~28px height in chrome (row hits are full 24px row width)
- Kind meaning never by color alone — always include the letter glyph
- `aria-pressed` on feed kind filters; `aria-label` on icon-only header controls
- Focus: accent-colored outline or input border; do not remove focus rings
- Open connection gaps exposed as `role="status"`

---

## Implementation notes

### Current code map (alpha)

| Concern       | Location                                             |
| ------------- | ---------------------------------------------------- |
| CSS tokens    | `src/index.css` `@theme`                             |
| Shell         | `src/App.tsx`                                        |
| Header        | `src/components/WorkspaceHeader.tsx`                 |
| Status        | `src/components/StatusBar.tsx`                       |
| Tree          | `src/components/FileTree.tsx`                        |
| Feed          | `src/components/ActivityFeed.tsx`, `src/lib/feed.ts` |
| Git panel     | `src/components/GitPanel.tsx`                        |
| Preview       | `src/components/Preview.tsx`                         |
| Layout widths | `src/stores/layoutStore.ts`                          |

### Token bridge (target → today’s names)

| Design token         | CSS / Tailwind                                                  |
| -------------------- | --------------------------------------------------------------- |
| `canvas` / `surface` | `--color-surface`, `bg-surface`                                 |
| `surface-elevated`   | `--color-surface-raised`                                        |
| `hairline`           | `--color-border`                                                |
| `hairline-strong`    | `--color-border-strong`                                         |
| `ink`                | `--color-text`                                                  |
| `body`               | `--color-text-body`                                             |
| `mute`               | `--color-text-muted`                                            |
| `ash`                | `--color-text-ash`                                              |
| `accent`             | `--color-accent`                                                |
| `kind-*`             | `--color-git-*`                                                 |
| `diff-*`             | `--color-diff-*`                                                |
| `glow`               | `--color-glow`                                                  |
| mono stack           | `--font-mono` via JetBrains Mono (`@fontsource/jetbrains-mono`) |

### Out of scope (alpha)

- Light / cream theme
- Marketing landing page system
- Non-mono decorative icon sets
- Large filled accent panels
- Custom illustrated empty states

---

## Iteration guide

1. Change **one component** at a time; match an existing neighbor (status bar
   vs feed toolbar) before inventing a third dialect.
2. Reference tokens by name (`{colors.kind-deleted}`, `{typography.stat}`).
3. New counts UI must use the **letter + number** pattern, not English summaries.
4. New borders default to `{colors.hairline}`; reach for elevated surface only
   for header/status/modals/inputs.
5. Before adding an icon font or SVG set, try a single mono glyph.
6. Keep semantic color off large backgrounds; tints for diff/glow only.
7. Preserve scrollport gutters when placing trailing row actions (pins, etc.).
8. When aligning with OpenCode-style systems: take **TUI grammar**, not cream
   marketing layout.

---

## Known gaps

- **Bracket markers** — `[+]` on the empty-state CTA is the only flourish; tree
  columns stay single-glyph for density.
- **Hover/focus matrix** not exhaustive per control; follow mute → ink on
  hover and accent border on focus.
- **Windows / Ubuntu** pixel parity for overlay scrollbars needs ongoing checks
  on trailing row actions.
- **No light theme** design yet — explicitly deferred.
- **Shiki theme** is still GitHub Dark Default; may drift slightly from the
  warm canvas ramp until a custom highlighter theme is chosen.
