# 06 · Run debugger, replay and run detail

## Run debugger modal (`RunHistory.dc.html`)

Opened from Run history (`?run=:id`). It is 1180×790, centred over the backdrop.

### Header

- Eyebrow "RUN · run_xxx" (mono ID).
- Workflow name (18/600) and a run status pill.
- Meta: "Started {time} · {duration} · {trigger} trigger · {n} retries".
- Actions: **Open run page** (secondary link to `/runs/:id`) and a close button (×).

### Body

- **Left, 300px: STEPS · n.** A numbered list of every node in execution order. Each row has the index (mono), a status dot (pulsing if running), the name (13/500), the kind (10px uppercase `--text-3`) and the duration (mono). The current step uses `--accent-soft`. Skipped and pending steps are at 60% opacity.
- **Right:**
  1. **Step timeline:** one segment per step (flex 1, 6px tall, 8px when current, radius 3, 3px gap), coloured by status (`--line-strong` for pending or skipped). The current segment has an accent ring. Each segment is a clickable button with `aria-label="Step 3: Enrich company, Retrying"`.
  2. **Step header:**
     - Eyebrow "STEP 3 OF 7 · HTTP REQUEST".
     - 26px category tile, name (16/600) and a status pill.
     - A note line, for example "503 on attempt 2 of 3 · next attempt in 40 s".
     - Right side: Duration and Attempts in mono.
  3. **INPUT | OUTPUT:** two JSON wells, 270px tall, each with a size label ("1.2 KB"). A failed output gets a `--fail` border and background. Use `// No data: this step did not run` for steps that didn't run.
  4. **LOGS:** a 104px mono well, labelled "Secrets redacted".

### Footer

- **← Previous step** and **Next step →**, disabled at the ends, with "Step n of N".
- **Pin input as test fixture** (secondary): saves this step's input as a fixture for Simulate mode.
- **Replay from this step** (primary): disabled for pending or running steps.

- **Opening behaviour:** select the first step whose status is failed, running, retrying, waiting or cancelled, otherwise step 1.
- **Keyboard:** ←/→ move between steps when focus is in the modal (handle the key on the dialog, not globally), and Esc closes.

### Replay dialog (nested, 600px)

- Eyebrow "REPLAY" and title "Replay from step {n} · {name}".
- Intro: "Starts a new run linked to {runId}. Steps 1–{n-1} are restored from their checkpoints and do not run again."
- **INPUT FOR THIS STEP** (RadioCards):
  - **Recorded input:** "Exactly what this step received in the original run".
  - **Edit input:** "Change the payload before replaying". This reveals a 110px mono JSON editor; validate it against the node's input schema.
- **RUN AGAINST** (RadioCards):
  - **Production · live vN:** "Real side effects; idempotency keys prevent duplicates where supported".
  - **Sandbox · draft vN:** "External calls recorded, not sent".
- Two boxes side by side:
  - **REUSED, NOT RE-SENT** lists side-effect steps before this one that succeeded.
  - **WILL RUN AGAIN** (`--warn-soft`) lists side-effect steps from this one onward. Each item says "deduped by idempotency key" where the connector supports it, otherwise "may send again". In sandbox, every item says "recorded only".
- Actions: **Cancel**, and **Start replay** or **Start sandbox replay**.
- After starting, show a green status banner inside the modal: "Replay started as run_xxx from step n · linked to run_yyy", with **Dismiss**. The new run appears at the top of Run history.

Side-effect node kinds are D1, R2, Queues, Email, Slack, GitHub, Square, PagerDuty and Discord. Better still, the connector manifest should declare `sideEffect: true` and `idempotent: boolean` per operation; use that when it's available.

## Run detail page (`RunDetail.dc.html`)

A full page for one run, linked from the modal.

- **Header:**
  - "← All runs" link, then the workflow name (26/600) and a status pill.
  - Meta: "Started 18 Nov, 02:00:04 Europe/London · 4 m 12 s · Schedule trigger · Revision v8 · corr_…" (mono).
  - Actions: **Export**, **Replay from start** and **Retry from checkpoint** (primary).
- **Left column:**
  - **Node timeline (Gantt).**
    - Columns: Node (260px: dot, name and note) | track | Time (80px, mono).
    - The track has quarter gridlines (`--line`) under a header showing "0 s · 1 m · 2 m · 3 m · 4 m".
    - Bars are 6px tall with radius 3, positioned by start and end time: success in `--ok`, backoff in `--warn`, failed attempts in `--fail`. Make each bar at least 0.5% wide.
    - The failing row gets a `--fail-soft` background, and skipped rows are at 55% opacity.
    - Legend below.
  - **Logs card** for the failing node, labelled "Secrets redacted".
- **Right, 340px: failed node panel.**
  - Eyebrow "FAILED NODE", name and connector/operation.
  - Error title in `--fail` and a one-line explanation.
  - **ATTEMPTS:** numbered list of status and backoff ("Retry-After 6 s", "retry budget exhausted").
  - Input summary with a link to the R2 artifact (mono `r2://…`).
  - A `--warn-soft` box: "{n} items already written. Retrying from the checkpoint re-sends batch 25 only and reuses its idempotency keys."
  - **RESUME FROM** radios: the recommended checkpoint, an earlier step, or Start (new run).
