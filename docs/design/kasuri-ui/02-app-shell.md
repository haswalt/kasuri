# 02 · App shell, routes and navigation

## Layout

```
┌──────────────┬──────────────────────────────────────────────────────┐
│ Sidebar 240  │ Page header 56 (breadcrumb · right-side status)      │
│              ├──────────────────────────────────────────────────────┤
│              │ Main: padding 32/40, column, gap 20–24               │
│              │   Eyebrow · H1 · one-line description   [actions]    │
│              │   Content (cards, tables, panels)                    │
└──────────────┴──────────────────────────────────────────────────────┘
```

- The **workflow editor** does not use the sidebar. It is full-bleed with its own 56px header and a 28px status bar (see `05`).
- **Sign in** is full-bleed with no shell.
- The page background is `--bg`. The sidebar and header are `--surface`, separated by a 1px `--line` border.
- The header breadcrumb reads `Workspace name / Page` with the parent in `--text-2` and the separator in `--text-3`, at 14px. The right side holds page status, for example "Live · updates every 5 s" with a pulsing `--run` dot on Runs, or the deployment host pill on Workflows.

## Routes

| Route | Screen | Nav item |
|---|---|---|
| `/login` | Sign in | n/a |
| `/setup` | Owner bootstrap with a setup token (not designed yet; reuse the sign-in layout) | n/a |
| `/w/:ws/workflows` | Workflows dashboard | Workflows |
| `/w/:ws/workflows/:id/edit` | Editor, Build mode | n/a (full-bleed) |
| `/w/:ws/workflows/:id/edit?mode=simulate` | Editor, Simulate mode | n/a |
| `/w/:ws/runs` | Run history; `?run=:runId&step=:n` opens the debugger modal | Runs |
| `/w/:ws/runs/:runId` | Run detail page | Runs |
| `/w/:ws/connectors` | Connector catalogue | Connectors |
| `/w/:ws/connections` | Connections; `?selected=:id&filter=mcp` | Connections |
| `/w/:ws/mcp` | MCP server | MCP server |
| `/w/:ws/audit` | Audit log (not designed yet; use the table pattern from Runs) | Audit log |
| `/w/:ws/settings/:section` | Settings: `profile`, `tokens`, `workspaces`, `members`, `deploy` | n/a (account menu) |

Put modal state in the URL (for example `?run=`) so runs can be deep-linked and the back button closes the modal.

## Sidebar (`Sidebar.dc.html`)

From top to bottom, with 20px padding top and bottom, 12px left and right, and a 20px gap:

1. **Brand row:** the woven mark (28px), the wordmark "kasuri" and an "alpha" pill (11px, `--surface-2` fill, 1px `--line` border, 4px radius).

2. **Workspace switcher:** a full-width button with a 1px `--line` border and 8px radius. It shows a 28px letter tile in the workspace colour, the workspace name (13px/500), a sub-line with role and workflow count ("Owner · 7 workflows") and a chevrons-up-down icon.
   - Clicking it opens a menu anchored below. The menu has a "WORKSPACES" eyebrow, then the workspaces with letter tile, name and role/count, and a check on the current one (`role="menuitemradio"`).
   - Below a divider: "Create workspace", then "Manage workspaces" (goes to `/settings/workspaces`).
   - Switching workspace changes `:ws` and reloads data. Nothing is shared across workspaces.

3. **Main nav** (`<nav aria-label="Main">`): Workflows (count badge), Runs, Connectors, Connections (red badge with the count of connections that need attention), MCP server, Audit log.
   - Items are 36px tall with 10px padding, a 16px icon and 14px text.
   - The active item uses `--accent-soft` background, `--accent` text and weight 500, with `aria-current="page"`. Inactive items use `--text-2`.
   - **Settings is not in the main nav.** It lives in the account menu.

4. Spacer.

5. **Account button** (bottom): avatar initials, name and email, and a chevron, styled like the workspace switcher. It opens a menu **upwards** (`bottom: 76px`) containing:
   - Header: name, then email · role.
   - **Settings** (to `/settings/profile`), **Members & roles** (to `/settings/members`), **Light theme** or **Dark theme** (toggles the theme), **Keyboard shortcuts** (`?`), **Documentation** (external).
   - A divider, then **Sign out** in `--fail`, which goes to `/login`.

   While on any Settings route, the account button gets an accent border, because Settings has no nav item.

- Only one sidebar menu can be open at a time. Close menus on outside click, on Escape and on navigation. Escape returns focus to the trigger button.
- Menus are 10px radius with a 1px `--line-strong` border, `--shadow-popover` and 6px padding. Items are 8px padding, 13px text and 6px radius, with a hover background of `--surface-2`.

## Global behaviours

- **Keyboard:** `/` focuses the current page's search, or the node search in the editor. `?` opens the shortcuts dialog. `Esc` closes the top-most popover, menu or modal.
- **Toasts** (bottom centre): dark, using `--text` as background and `--bg` as text, 8px radius. A message plus an optional **Undo** action. Auto-dismiss after 6s and pause on hover. Use them for reversible actions (pause, deactivate, delete).
- **Live data:** Runs and the dashboard refresh every 5s, or use SSE/WebSocket if available. Don't reflow the table while a row menu is open.
