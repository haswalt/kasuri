# 01 · Foundations

## Theme model

- Themes: `light` (default), `dark`, `system` (follows `prefers-color-scheme`).
- Set `data-theme="light|dark"` on `<html>`. For `system`, remove the attribute; `tokens.css` handles the media query.
- Save the user's choice (Settings › Profile & security › Theme, and the account menu toggle) per user. Fall back to `system`.
- Every colour comes from a CSS variable in `tokens/tokens.css`. Component CSS must never contain raw hex, except for the logo.

## Colour tokens

| Token | Light | Dark | Use |
|---|---|---|---|
| `--bg` | `#F7F8FA` | `#0B0D11` | Page background behind panels |
| `--surface` | `#FFFFFF` | `#111419` | Cards, sidebar, header, panels, inputs |
| `--surface-2` | `#F2F3F6` | `#181C23` | Table headers, code/JSON wells, segmented-control track, subtle fills |
| `--canvas` | `#F4F5F8` | `#0D1015` | Workflow canvas background |
| `--dot` | `#D3D7DF` | `#22272F` | Canvas dot grid (1 px dots on a 20 px grid) |
| `--line` | `#E5E7EB` | `#1F242C` | Default 1 px borders and dividers |
| `--line-strong` | `#D0D4DC` | `#2E343E` | Input and button borders, idle edges, node borders |
| `--text` | `#181B21` | `#E6E8EC` | Primary text |
| `--text-2` | `#525967` | `#A3ABB9` | Secondary text, inactive nav and tabs |
| `--text-3` | `#6B7280` | `#8B93A3` | Tertiary text: eyebrows, timestamps, placeholders |
| `--accent` | `#0B6E79` | `#4CC3CB` | Primary buttons, selection, active nav, links |
| `--accent-fg` | `#FFFFFF` | `#061A1C` | Text on accent |
| `--accent-soft` | `#E4F2F3` | `rgba(76,195,203,.14)` | Active nav and tab background, selected row, focus ring |

### Status colours

Each status has a foreground (`--x`) and a soft background (`--x-soft`).

| Status | Token | Light fg / bg | Dark fg | Meaning |
|---|---|---|---|---|
| Succeeded / healthy / active | `--ok` | `#1A7F37` / `#E9F5EC` | `#3FB950` | Completed, connected, live |
| Running | `--run` | `#0969DA` / `#E4EFFC` | `#58A6FF` | In progress; animated edges in simulate mode |
| Retrying / paused / expiring | `--warn` | `#9A6700` / `#FBF2DD` | `#D29922` | Needs attention soon; backoff |
| Failed / destructive | `--fail` | `#CF222E` / `#FCECED` | `#F85149` | Error, revoke, delete |
| Waiting / ask first | `--wait` | `#8250DF` / `#F3EEFD` | `#A78BFA` | Waiting for approval or a timer |
| Skipped / cancelled / queued | `--skip` | `#6E7781` | `#768390` | Not run, branch not taken |

### Node category colours

These are used for icon tiles on nodes, palette items and connector cards: `--c-{cat}` is the icon colour and `--c-{cat}-bg` the tile background.

| Category | Light | Dark | Examples |
|---|---|---|---|
| `trigger` | `#7C5CD6` / `#F1EDFB` | `#A78BFA` | Webhook, Schedule, Manual, Cloudflare Email trigger |
| `http` | `#2563EB` / `#E8EFFD` | `#7DA6FF` | HTTP Request, AI models, MCP |
| `logic` | `#475467` / `#EEF0F3` | `#B4BCC8` | IF, Switch, Merge, Filter, Loop, Map/Transform |
| `cf` | `#B4590F` / `#FCEFE3` | `#FB923C` | D1, R2, KV, Queues, Durable Objects |
| `app` | `#A8436A` / `#F9ECF1` | `#F472B6` | Slack, GitHub, Google, Square, Discord |
| `error` | `#CF222E` / `#FCECED` | `#F85149` | Error Handler, PagerDuty, Sentry |

## Typography

- **Inter** for all UI text. **JetBrains Mono** for IDs, URLs, cron strings, code, JSON, expressions, durations in timelines and API scopes.
- Use tabular numbers (`font-variant-numeric: tabular-nums`) for counts and durations in tables.

| Role | Size / weight | Notes |
|---|---|---|
| Page title (h1) | 28px / 600, letter-spacing −0.025em | One per page |
| Modal or section title | 15–18px / 600 | |
| Body | 14px / 400 | Default UI text |
| Table and dense UI | 13px / 400–500 | Tables, panels, inspector |
| Secondary | 12px / 400, `--text-3` | Timestamps, hints |
| Eyebrow | 11px / 500, uppercase, letter-spacing 0.10–0.14em, `--text-3` | Above page titles ("BUILD & AUTOMATE") and panel section labels ("INPUT", "OUTPUTS") |
| Node kind label | 10px, uppercase, letter-spacing 0.08em, `--text-3` | Above the node title; turns into the status word in simulate mode |
| Mono | 11.5–12.5px | |

## Spacing, radius, elevation

- Spacing is on a 4px base. Common values: 4, 6, 8, 10, 12, 14, 16, 18, 20, 24, 28, 32, 40.
- **Page padding:** 32px top/bottom, 40px left/right. **Header height:** 56px. **Sidebar width:** 240px. **Footer and status bar:** 28px.
- **Radius:** 4 (chips, kbd), 5–6 (pills, small buttons, menu items), 7–8 (buttons, inputs, nodes), 10 (cards, panels, menus), 12 (modals).
- **Borders:** 1px everywhere. A selected node uses a 1px accent border plus a 3px `--accent-soft` ring (box-shadow), not a thicker border.
- **Elevation:** cards use `--shadow` (`0 1px 2px rgba(16,24,40,.04)` light, none in dark). Menus and popovers use `0 12px 32px rgba(12,16,24,.16)`. Modals use `0 24px 64px rgba(12,16,24,.28)` over a `rgba(12,16,24,.45)` backdrop.

## Iconography

- Lucide-style 24px stroke icons, rendered at 12–18px with a 1.6–2 stroke width and `currentColor`. If the app already uses `lucide-react`, use these:

| Concept | Lucide icon |
|---|---|
| Workflows | `Diamond` |
| Runs | `ArrowUpRight` |
| Connectors | `LayoutGrid` (square split into four) |
| Connections | `KeyRound` / `Key` |
| MCP server | `Plug` |
| Audit log | `FileText` |
| Settings | `SlidersHorizontal` |
| Webhook trigger | `Zap` |
| Schedule | `Clock` |
| IF / Switch | `GitBranch` |
| Map / Transform | `Command` |
| HTTP | `ArrowUpRight` |
| D1 / databases | `Database` |
| Queues | `Layers` |
| R2 | `Box` |
| Slack / channels | `Hash` |
| Error handler | `TriangleAlert` |
| AI / agent | `Sparkles` |
| Search | `Search` |

- Icon-only buttons need `aria-label`. Decorative icons get `aria-hidden="true"`.

## Logo

- The **woven mark** is a 2×2 plain weave of four ribbons. Files: `assets/kasuri-mark.svg` (32×32 grid) and `assets/kasuri-lockup.svg`.
- The wordmark is lowercase "kasuri" in Inter 600 (19px in the sidebar) with −0.02em letter-spacing, followed by a small "alpha" pill.
- The mark uses fixed colours that work on light and dark: weft `#4CC3CB`, weft fold `#2A9AA4`, warp `#138A96`, warp fold `#0B6470`, and shadows of `#04363C` at 35% opacity. Don't recolour it.
- Sizes: 28px in the sidebar and editor header, 34px on sign-in. Minimum 16px.

## Motion

- Durations: 150ms for hover and press, 250ms for state changes (node status, border colour), 300ms for progress bars.
- Animations, defined in `tokens.css`:
  - `kasuri-flow` animates a dashed stroke on active edges.
  - `kasuri-pulse` is used on "running" dots.
- Wrap all decorative motion in `@media (prefers-reduced-motion: no-preference)`, or disable it under `reduce`.
