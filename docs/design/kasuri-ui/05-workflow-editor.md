# 05 · Workflow editor (Build and Simulate modes)

Route: `/w/:ws/workflows/:id/edit[?mode=simulate]`. Reference: `Editor.dc.html`, which supports `mode="build|simulate"`. There is **one screen with two modes**. Switching modes keeps the canvas, header and node positions; only the side panels, a toolbar strip and node styling change.

## Frame

```
Header 56 ─ logo · ← All workflows │ Name · [Draft v13] · Saved 12 s ago ……… [Build|Simulate] │ (●) Live v12 active · Export · Publish v13
[Simulate only] Toolbar 48, --accent-soft background
Body ─ Left panel 240 │ Canvas (flex) │ Right panel 320
Status bar 28
```

### Header

- The **mode switch** is a SegmentedControl with "Build" (pencil icon) and "Simulate" (play icon). It is reflected in the URL (`?mode=`).
- The **live switch** (role `switch`) is labelled "Live v12 active" and activates or deactivates the published version. It is separate from the draft.
- **Publish v13** is the primary action. **Export** downloads the workflow JSON.

## Build mode

### Left: node palette

- A search box (`/` focuses it).
- Grouped sections with an eyebrow and count: TRIGGERS, LOGIC, CLOUDFLARE, DATA & UTILITIES, APPS, AI. Items are 30px rows with a 22px category tile and the name. Drag them onto the canvas, or click to add after the selected node.

### Canvas

- `--canvas` background with a 20px dot grid of `--dot`. Use **XYFlow / React Flow** if the app already does; the geometry below is what the design assumes.
- The top-left eyebrow is "CANVAS" and the top-right shows "{n} NODES · {m} EDGES".
- **Nodes:** the 200×56 node card (see `03`). Lay them out top-to-bottom: vertical spacing 112px between node tops, 160px between columns for branches.
- **Edges:**
  - 1.25px `--line-strong` lines with small open arrowheads (8px wide, 6px deep).
  - Vertical straight lines, or cubic beziers from the bottom-centre of the source to the top-centre of the target for branches.
  - **Error edges** leave the side of the node, dashed `4 4`, in `--fail` at 70% opacity.
  - **Edge labels** (`true`, `false`, `error`) are mono 10.5px pills on `--surface` with a 1px `--line` border, centred on the edge midpoint. `true` is in `--ok`, `false` in `--text-2` and `error` in `--fail`.
- Zoom control at bottom-left: − · 100% · +. A hint at bottom-right: "Drag from the palette, or press / to search".

### Right: node configuration

The inspector is generated from the node's schema (JSON Schema / Zod metadata).

- **Header:** "Node configuration", the node ID (mono, `--text-3`), and the type plus version in `--accent` (for example "IF / Condition · v2").
- **Underline tabs:** Parameters · Settings (retry, timeout, error routing) · Output (last test output).
- **Parameters for IF:**
  - Node name.
  - Value: an expression in mono on `--surface-2`, with a resolved preview below ("Resolves to 82 with the pinned 'Hot lead' input").
  - Operator and Compare to, side by side.
  - "+ Add condition".
  - "If the value is missing".
  - OUTPUTS: `true` / `false` with their target and item count.
  - A schema note box.
  - **Remove node** (danger-link).

### Status bar

"● Graph valid" (or an error count that links to the problem) · credentials connected · last test result · "Test webhook URL is isolated from production".

## Simulate mode

The draft runs against a **saved test input** in a sandbox. External calls are **recorded, not sent**, and D1 writes go to a scratch database.

### Toolbar (48px, `--accent-soft`)

- "SIMULATION" eyebrow in `--accent`, followed by "Draft v13 runs in a sandbox · external calls are recorded, not sent".
- **Play/Pause/Replay** (primary). The label is "Play", "Pause", or "Replay" once finished.
- **Step**: advances one event.
- **Reset**.
- **Speed** segmented: 0.5×, 1×, 2×. The base tick is 1000 ms ÷ speed.
- Right side: "Step n of N" with elapsed ms and a 3px progress bar, plus a summary pill: Ready / Running / Succeeded / "Failed · handled".

### Left: test inputs (replaces the palette)

- A "TEST INPUTS" eyebrow and a one-line explainer.
- Fixture cards. Each card has a name, a tag chip (for example "score 82", or "mock 503" in `--fail`) and a mono payload preview. The selected card has an accent border and ring. Picking one resets the simulation.
- "+ Pin an input from a past run" links to Runs, where any step input can be pinned.

### Canvas styling by step status

| Status | Border | Ring / extra | Kind label |
|---|---|---|---|
| idle | 1px `--line-strong` | `--shadow` | node kind |
| running | 1px `--run` | 3px `--run-soft` | "RUNNING" in `--run` |
| retrying | 1px `--warn` | 3px `--warn-soft` | "RETRYING" in `--warn` |
| success | 1px `--ok` | none | "SUCCEEDED" in `--ok` |
| failed | 1px `--fail` | none | "FAILED" in `--fail` |
| skipped | 1px dashed `--line-strong` | opacity .45 | "SKIPPED" |
| selected (any) | n/a | `outline: 1.5px solid var(--accent); outline-offset: 4px` | n/a |

Edges:
- **Active**, meaning the next event's incoming edge: `--run`, 1.75px, dashed `5 5`, animated with `kasuri-flow .7s linear infinite`.
- **Done:** `--ok` (`--fail` for error edges).
- **Skipped:** `--line`, dashed `4 4`.

Clicking a node selects it for the right panel. The "···" overflow is hidden in this mode. A hint card at the top-left of the canvas appears only before the first step.

### Right: execution (replaces the inspector)

- **Execution log**, with the sim ID in mono: an ordered list of applied events. Each row has a 7px status dot, the node title (13/500), a message (12px `--text-2`) and `+{t} ms` (mono). It ends with a "Run finished" row.
- **Selected node data:** title and status note, an INPUT well and an OUTPUT well, each 96px tall and scrollable. Show `// Not reached yet` or `// Skipped: this branch was not taken` where relevant.

### Status bar

"D1 writes go to a scratch database · Slack, Queues and PagerDuty calls are recorded" on the left, and "{n} side effects recorded: …" on the right.

### Simulation state machine

The backend returns an ordered event list for a fixture. Ideally it streams it; the UI only replays it.

```ts
type SimEvent = {
  nodeId: string;
  status: 'success' | 'retrying' | 'failed';
  via?: string;      // edge id that led here (marks it done)
  message: string;   // shown in the log
  tMs: number;       // ms since start
  skip?: string[];   // node + edge ids that become skipped (branch not taken / upstream failed)
  input?: unknown;   // redacted payloads for the data panel
  output?: unknown;
};
state = { fixtureId, step /* events applied */, playing, speed, selectedNodeId }
```

- **Derived:** applying `events[0..step)` gives node statuses, done edges and skipped items. `next = events[step]`. When `playing || step > 0`, the next node shows **running**, or **retrying** if it already has a retrying status, and `next.via` shows as the active edge.
- **Play:** schedule `step++` every `1000/speed` ms until `step === events.length`. Pause clears the timer. Changing speed reschedules. Step pauses and increments. Reset sets step to 0 and clears selection.
- **Default selection** is the last applied event's node.
- **Reference fixtures** (see the `events()` method in `Editor.dc.html`):
  - **Hot lead:** 429 retry, then success, true branch, D1 and Slack.
  - **Cold lead:** false branch to the queue.
  - **Enrich API down:** 503 three times, then the error handler.

## Accessibility

- Every node is a `<button>` with `aria-label="{title}, {status}"`. The canvas container has `aria-label="Workflow canvas"`.
- Toolbar controls are real buttons, and the speed and fixture pickers use `aria-pressed`.
- Respect reduced motion: no edge animation and no pulsing, but status colours still change.
