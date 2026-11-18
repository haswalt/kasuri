# 03 · Components

Build these once and reuse them. All sizes are in px. Colours are tokens from `01-foundations.md`.

## Button

| Variant | Style |
|---|---|
| `primary` | `--accent` background, `--accent-fg` text, weight 500, no border |
| `secondary` | `--surface` background, 1px `--line-strong` border, `--text` |
| `ghost` / link | Transparent, `--accent` text (or `--text-2`), no padding |
| `danger` | `--fail` background, white text (confirm dialogs only) |
| `danger-link` | Transparent, `--fail` text (Remove node, Revoke, Sign out) |

- Sizes: `sm` 28–30 h, 13px, padding 0 10–12. `md` 34–36 h, 13–14px, padding 0 14. `lg` 46–52 h (sign in, hero).
- Radius 7–8. An optional leading icon is 12–16px with a 6–8px gap.
- Disabled: `--text-3` text and 0.6 opacity, no hover change, `aria-disabled`.
- The deploy button uses a dark `#11141A` background. This is used on marketing only and is not in app scope.

## StatusPill

`<StatusPill tone="succeeded" />` renders a 6px `currentColor` dot and a label. It is 12px text, padding 3px 8px, radius 5, with `color: tone.fg; background: tone.bg`. Tones come from `tokens.ts` maps (`runStatusTone`, `stepStatusTone`, `workflowStatusTone`, `connectionTone`). A running pill's dot gets `kasuri-pulse`.

## StatusDot

A 6–7px circle in `tone.fg`. Use it in tables ("Last run"), step lists and logs. Running dots pulse.

## Eyebrow

11px uppercase text with 0.1–0.14em letter-spacing in `--text-3`. It sits above page titles and labels sections inside panels.

## PageHeader

Eyebrow, then H1 (28/600) with a 6px gap, then a one-line description (14px `--text-2`). Actions sit on the right, bottom-aligned: secondary actions first, primary last.

## Card / Panel

`--surface` background, 1px `--line` border, 10px radius, `--shadow`. A panel section header is 12–14px padding with a 14/600 title, an optional 12px `--text-3` sub-line, and a bottom border. Inside cards, sections are separated by 1px `--line` dividers, not by nested cards.

## StatStrip

A card containing an equal-width grid of 4 cells, separated by 1px `--line` left borders. Each cell has a label (13px `--text-2`), a value (26/600, tabular) and a sub-line (12px `--text-3`). An optional footer row holds a stacked status bar (6px tall, radius 3, 2px gaps between segments, each segment at least 0.8% wide) and a legend of dot, label and count.

## Tabs (filter tabs)

Buttons that are 30 tall with 0 10 padding and radius 6. Active: `--accent-soft` background, `--accent` text, weight 500. Inactive: transparent, `--text-2`. An optional count follows the label at 11px (`--text-3`, or `--accent` when active). Use `aria-pressed` for filters, or `role="tab"` when the tabs swap panels.

## Underline tabs

Used in the editor inspector, MCP connect panel and Settings sub-panels. Text is 13px, 36–40 tall and has no padding, with a 1.5px bottom border (`--accent` when active, transparent otherwise) and an 18px gap. They sit on a 1px `--line` rule.

## SegmentedControl

A track with 2px padding, 7–8px radius, 1px `--line` border and `--surface-2` background. Options are 24–28 tall. The active option gets a `--surface` background, 1px `--line` border, `--text` colour and weight 500. Used for Build | Simulate, Visual | TypeScript | JSON, speed (0.5× 1× 2×) and theme.

## Table

- Full width, 13px text.
- Header row: `--surface-2` background, 12px/500 `--text-3`, padding 9px 12–16px, with top and bottom 1px `--line`.
- Rows are 46–58 tall, padding 12–16px horizontally, with a bottom border. Hover: `--surface-2`.
- A selected row uses `--accent-soft`.
- The primary cell is name (500) plus a sub-line (12px `--text-3`). IDs are in mono.
- Footer: 12px `--text-3` summary on the left and paging buttons on the right.
- Numbers are right-aligned and tabular.

## Search input

32–36 tall with a 1px `--line-strong` border, 7px radius and a 14px search icon. The input itself has no border. An optional `/` kbd hint sits at the right. Give it a visually hidden `<label>`.

## Form fields

The label (13px `--text-2`) sits above the control with a 6px gap. Inputs and selects are 36 tall, 7px radius, 1px `--line-strong` border and `--surface` background, with 0 12 padding. Expression fields use mono 12px on a `--surface-2` background. A hint below is 12px `--text-3`. Errors show a `--fail` border and a 12px `--fail` message.

## Menu / Popover

10px radius, 1px `--line-strong`, `--shadow-popover`, 6px padding, 260–280 wide. Items have a 13px label and an optional 11.5px `--text-3` hint line. Destructive items are `--fail` and sit last, below a divider. Use `role="menu"` and `menuitem`, arrow-key navigation, Esc to close, and return focus to the trigger. The **row action menu** ("···" button, 28×28) opens upwards for rows in the bottom half of the viewport.

## Modal / Dialog

The backdrop is `--backdrop`; clicking it closes the dialog, except for destructive confirmations. The dialog has a `--surface` background, 1px `--line`, 12px radius and `--shadow-modal`. Use `role="dialog"` (or `alertdialog` for destructive), `aria-modal`, a focus trap, Esc to close and focus restore.

- **Confirm destructive** (for example "Delete workflow"): 460 wide, title, bullet list of consequences, a "Type the name to confirm" input, Cancel, then a `danger` action. The action stays disabled until the typed name matches.
- **Large modal** (run debugger): 1180×790, see `06`.
- **Nested dialog** (replay): 600 wide, layered inside the large modal with its own backdrop.

## RadioCard

A full-width button with 9px 12px padding, 8px radius and 1px `--line` border (`--accent` when selected). It holds a 14px radio dot (a 4px accent ring when selected), a label and a 12px `--text-3` hint. Use `role="radio"` and `aria-checked` inside a `fieldset`/`legend`.

## Switch

30×18 track with a 14px knob. On: `--ok` track. Use `role="switch"` and `aria-checked`. The label sits to the right (13px `--text-2`).

## Toast

See `02-app-shell.md`.

## JSON / log well

A `<pre>` in mono at 11.5–12px with 1.5–1.6 line height, on `--surface-2` with a 1px `--line` border, 7–8px radius and 8–12px padding. It has a fixed height and scrolls. A failed output uses a `--fail` border and `--fail-soft` background. Secrets are always rendered as `••••••••`. The UI must never receive raw secrets.

## Node card (canvas)

Used in the editor, in simulate mode and in the run debugger list styling.

- 200×56, 8px radius, `--surface` background, 1px `--line-strong` border, `--shadow`.
- Contents: a 30px category tile (7px radius, `--c-{cat}-bg` background, 15px icon in `--c-{cat}`), a kind label (10px uppercase `--text-3`) above the title (13/500), and a "···" overflow on the right in Build mode.
- Selected (Build): 1px `--accent` border plus a 3px `--accent-soft` ring.
- In simulate mode, see the status table in `05`.

## CategoryTile

Square with radius ≈ size/4, background `--c-{cat}-bg`, icon in `--c-{cat}`. Sizes: 22 (palette), 30 (nodes), 32 (connector cards and table rows), 36 (detail headers).

## Logo

`<KasuriMark size={28} />` renders `assets/kasuri-mark.svg` inline. `<KasuriLockup />` renders the mark, the wordmark and the "alpha" pill.
