# Kasuri UI handoff pack

This pack describes the new Kasuri UI so it can be built into the existing React app. It covers the application screens only. **The marketing site is out of scope** (it is still being redesigned).

- **Design canvas (source of truth for visuals):** https://claude.ai/artifact/QJWvVqjUtvuz74deZGxGJ1
- **Reference prototypes:** `reference/*.dc.html`. These are design prototypes with inline styles and fake data. Port the *look and behaviour*, not the code. Each file's `<script>` block contains the sample data and interaction logic used in the design, which is useful for fixtures and for checking behaviour.

## What's in the pack

| File | Use it for |
|---|---|
| `01-foundations.md` | Colour tokens, type, spacing, radii, icons, motion, light/dark theming |
| `tokens/tokens.css` | Drop-in CSS custom properties for light, dark and system themes, plus shared keyframes |
| `tokens/tokens.ts` | The same tokens as a typed object (status and category maps included) |
| `02-app-shell.md` | Layout, routes, sidebar, workspace switcher, account menu |
| `03-components.md` | Shared primitives: buttons, pills, tabs, tables, menus, dialogs, toasts, node cards |
| `04-screens.md` | Every screen: layout, data, interactions, empty/loading/error states |
| `05-workflow-editor.md` | Editor build mode and simulate mode, canvas geometry, simulation state machine |
| `06-run-debugger.md` | Run history modal, step-through debugging, replay dialog, run detail page |
| `types/kasuri-ui.ts` | TypeScript contracts for everything the UI renders |
| `07-acceptance.md` | Definition of done and QA checklist |
| `assets/` | Woven logo mark and lockup as SVG |
| `reference/` | Prototype sources, one per artboard (see table below) |

## Suggested build order

1. **Foundations.** Add `tokens.css`, fonts (Inter 400/500/600/700, JetBrains Mono 400/500) and theme switching (`data-theme` on `<html>`). Replace any hard-coded colours.
2. **Primitives.** Build the components in `03-components.md`, with Storybook or a scratch route if the app has one.
3. **App shell.** Sidebar with the workspace switcher and account menu, page header, routes.
4. **Workflows dashboard.** Includes row actions and the delete confirmation.
5. **Workflow editor in Build mode,** then **Simulate mode** (same screen, see `05`).
6. **Runs.** Run history, then the run debugger modal and replay dialog, then the run detail page.
7. **Connectors, Connections** (including MCP servers your agents use), **MCP server**.
8. **Settings** (opened from the account menu, not the main nav).
9. **Sign in.**
10. Dark-mode pass and the `07-acceptance.md` checklist.

Keep each step shippable. Wire screens to the existing API where it already exists; otherwise use typed fixtures from `types/kasuri-ui.ts` behind a clear `// TODO(api)` marker.

## Ground rules

- **Desktop only.** Design width is 1440×900; support down to 1280 px wide. No mobile layouts.
- **Thin and calm.** Use 1 px borders, white surfaces on a light grey page, one accent colour (petrol teal), and colour only where it carries meaning (status, node category). No gradients, heavy shadows or emoji.
- **Tokens, not hex.** Components read colours from CSS variables only, which is what makes dark mode work. The only fixed hex values are the logo colours.
- **One source per concept.** Status colours, node categories and policies come from the maps in `tokens.ts`; don't redefine them per screen.
- **Accessible by default.** Use real `<button>`/`<a>`/`<input>` elements, visible focus, `aria-*` on menus and dialogs, text contrast ≥ 4.5:1 (the tokens already pass), and honour `prefers-reduced-motion`.
- **Product terms.** A *workflow* has *nodes*; a *run* has *steps*. *Connectors* are the catalogue of integrations; *connections* are saved accounts and credentials. *Simulate* means a sandbox run of a draft in which external calls are recorded, not sent.

## Reference files → screens

| Reference file | Screen |
|---|---|
| `Main.dc.html` | Sign in |
| `Dashboard.dc.html` | Workflows dashboard |
| `RunHistory.dc.html` | Runs list, run debugger modal and replay dialog |
| `RunDetail.dc.html` | Run detail page |
| `Editor.dc.html` | Workflow editor (Build and Simulate modes; `mode` prop) |
| `Simulator.dc.html` | Editor opened in Simulate mode (wrapper only) |
| `Connectors.dc.html` | Connector catalogue |
| `Connections.dc.html` | Connections, including MCP servers |
| `MCP.dc.html` | MCP server (Kasuri as a tool provider) |
| `Settings.dc.html` | Settings (5 sections) |
| `Sidebar.dc.html` | Shared sidebar component |
| `Brand.dc.html` | Logo usage |
| `*Dark.dc.html` | Same screens with `theme="dark"`; no separate design |

The marketing site prototype is deliberately not included.
