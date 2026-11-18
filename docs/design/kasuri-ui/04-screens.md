# 04 · Screens

Every screen below has a reference in `reference/`. Sample data in the prototypes is illustrative only.

For **every** list or panel, provide:
- **Loading:** skeleton rows at the real row height, with no spinners in tables.
- **Empty:** a one-line explanation and a primary action.
- **Error:** an inline message in a `--fail-soft` panel with "Try again".

---

## Sign in · `/login` · `Main.dc.html`

- Two columns. The left (800px) is `--canvas` with a dot grid and the logo lockup. It shows a decorative mini-workflow with four nodes (two succeeded, one running with a flowing edge, one skipped and dashed), the headline "Durable workflows, on your own Cloudflare account." and three check-marked benefits.
- The right column (`--surface`) holds a 380px form:
  - "Sign in to flows.example.com", using the deployment host.
  - Email and Password fields, with a "Forgot password?" link.
  - "Keep me signed in on this device" checkbox.
  - Primary **Sign in**, an "or" divider, then a secondary **Continue with SSO (OIDC)**, shown only if OIDC is configured.
  - A note box: "First deployment? Create the owner account with the bootstrap token…" linking to `/setup`.
  - Footer: health dot, version and "Open source".
- Errors show inline under the form ("Email or password is incorrect"). Lock the button while the request is in flight.

## Workflows dashboard · `/w/:ws/workflows` · `Dashboard.dc.html`

- **PageHeader:** eyebrow "BUILD & AUTOMATE", title "Workflows", description "{n} workflows · {active} active · run states for the last 24 hours". Actions: **Import JSON** (secondary) and **+ New workflow** (primary, opens the editor on a new draft).
- **StatStrip:** Runs (24h), Success rate (with the change against yesterday), Failed (with the dead-letter count), Awaiting approval (with the oldest wait). Below it, a stacked run-state bar with legend: Queued, Running, Waiting, Retrying, Succeeded, Failed, Cancelled.
- **Workflow table card:**
  - Toolbar: filter tabs (All, Active, Draft, Paused) and a search box.
  - Columns:
    - **Workflow:** 32px diamond tile, name and description; the name links to the editor.
    - **Status:** a `workflowStatusTone` pill.
    - **Last run:** status dot and label, linking to the run, with relative time and duration below.
    - **Runs 24h:** right-aligned.
    - **Trigger:** mono, for example `POST /hooks/leads` or `cron 0 2 * * *`.
    - **Updated.**
    - **Actions:** "···".
  - Footer: "Showing x of y" and a "View run history" link.
- **Row actions menu.** The available actions depend on status:

| Status | Actions (label · hint) |
|---|---|
| Active | **Pause**: stop starting runs; events queue for up to 7 days · **Deactivate**: remove triggers; new events are rejected |
| Paused | **Resume**: start runs again, then drain queued events · **Deactivate** |
| Inactive | **Activate**: register triggers for the live version |
| Draft | **Publish & activate**: publish as v1 and register triggers |
| Always | **Run now** · **Duplicate** · **Export JSON** · divider · **Delete…** (danger) |

  - Status changes are optimistic. Show a toast with **Undo**, and roll back on API error.
  - **Delete…** opens the destructive confirm. It lists that triggers are removed, runs in progress are cancelled at their next checkpoint, history is kept for 30 days and the workflow can be restored for 30 days. The user types the name to enable the button. After delete, a toast offers Undo.

## Run history · `/w/:ws/runs` · `RunHistory.dc.html`

- **PageHeader:** eyebrow "OBSERVE", title "Runs", description "Every execution across your workflows. Select a run to step through it node by node." Action: **Export CSV**.
- **Card:**
  - Status tabs with 24h counts: All, Running, Waiting, Retrying, Failed, Succeeded, Cancelled.
  - Filter row: Workflow select, Trigger select, Time range select and search ("Run ID, correlation ID or payload key").
  - Columns:
    - **Run:** mono ID, a button that opens the debugger.
    - **Workflow:** name (also opens the debugger), with an optional sub-line explaining the state (for example "Enrich company · 503 · next attempt in 40 s").
    - **Status:** pill.
    - **Trigger.**
    - **Started.**
    - **Duration:** mono, right-aligned.
    - **Nodes:** "3 / 7", right-aligned.
    - **Retries:** right-aligned.
  - Footer: "Showing n of N runs · newest first" with Previous/Next. Paginate with a cursor.
- Clicking a run sets `?run=` and opens the **run debugger modal** (see `06`).

## Run detail · `/w/:ws/runs/:runId` · `RunDetail.dc.html`

See `06-run-debugger.md`.

## Connectors · `/w/:ws/connectors` · `Connectors.dc.html`

- **PageHeader:** eyebrow "LIBRARY", title "Connectors", description "The nodes and integrations available to your workflows. Each is versioned and tested with Kasuri." Action: **New HTTP connector** (secondary).
- Category tabs with counts: All 50, Core 6, Logic & data 18, AI 9, Cloudflare 6, Apps 8, Monitoring 3. The search box sits on the right.
- **Card grid** (4 columns, 14px gap, cards at least 148 tall):
  - A 32px category tile and a state badge at the top right:
    - **Connected / "1 connection"** (ok)
    - **Reconnect** (warn)
    - **Built in** (accent; Cloudflare bindings and core nodes)
    - **Not connected** (neutral)
  - Eyebrow category, name (14/500), and a one-line description (12px `--text-2`).
  - Capability chips (11px, 1px `--line`, radius 4): auth method, Trigger, Actions, Tools.
  - Clicking a card opens the connector detail. That isn't designed yet; for now, route to Connections filtered to the connector, or to a "New connection" flow.
- Footer strip (dashed border, `--accent-soft`): "Need a service that isn't listed? Describe it with an OpenAPI document. No server-side code runs." and "Showing x of y".
- **MCP Client** appears here as a connector. Adding one creates a connection of type MCP (see Connections).

## Connections · `/w/:ws/connections` · `Connections.dc.html`

- **PageHeader:** eyebrow "CREDENTIALS", title "Connections", description "Accounts and keys your workflows use. Secrets are encrypted with a key held in Workers Secrets and never shown again." Action: **+ New connection** (primary).
- Two columns: the list card (flexible) and a details panel (360px).
- **List:**
  - Tabs: All, Needs attention (count in `--fail`), OAuth, **MCP servers**. Search box.
  - Columns:
    - **Connection:** tile, name, and a mono account or host line.
    - **Auth:** for example "OAuth 2.0 + PKCE", "API key" or "MCP · OAuth 2.1".
    - **Used by:** count, right-aligned.
    - **Status:** `connectionTone` pill with a specific label, for example "Refresh failing" or "Expires in 6 days".
    - **Last used.**
  - Rows are 50 tall. The selected row is `--accent-soft`; clicking a row selects it (`?selected=`).
- **Details panel** for the selected connection:
  - Header: 36px tile, name, mono account and a status pill.
  - **Alert box** when in warn or fail state, explaining what happened and how many workflows are affected. It is `--fail-soft` or `--warn-soft`.
  - Definition list: Auth, Created, Token (state or expiry), Key version (mono).
  - **For MCP connections:** a "TOOLS AGENTS MAY CALL" list. Each tool row has a mono name, a one-line description, a kind chip (`read` neutral, `write` warn, `destructive` fail) and a **policy button** that cycles Auto → Ask first → Blocked (`toolPolicyTone`). Policy changes save immediately and are audited.
  - **SCOPES:** mono chips.
  - **USED BY:** workflow links to the editor.
  - Actions: **Reconnect** (primary, when failing) or **Edit**, **Test**, and **Revoke** (danger-link, with confirmation).

## MCP server · `/w/:ws/mcp` · `MCP.dc.html`

This page is only about **clients connecting to Kasuri**. Outbound MCP servers are connections.

- **PageHeader:** eyebrow "MODEL CONTEXT PROTOCOL", title "MCP server". The description links to Connections: "Let Claude and other MCP clients connect to Kasuri and call your workflows as tools. To give your own agents tools from other MCP servers, add them under Connections." Action: **Expose a workflow**.
- **StatStrip:**
  - Endpoint: mono URL with a copy button.
  - Transport: "Streamable HTTP".
  - Auth: "OAuth 2.1 or client token".
  - Status: "Enabled · {visible} of {n} tools visible".
- **Workflows exposed as tools** table: Tool (mono), Workflow, Input (mono schema summary such as `{ email, company, source }`), Policy (cycling button), Calls 24h. Footer hint: "Auto runs straight away · Ask first waits for an approval · Blocked hides the tool".
- **Clients allowed to connect:** rows show name, how (OAuth user or token prefix), scope, last used and **Revoke**. Header action: **Issue client token**. The token is shown once in a dialog.
- **Connect a client** side panel (380px), with underline tabs **Sign in (OAuth)** and **Client token**:
  - Numbered steps.
  - A dark code block: the URL for OAuth, or a JSON `mcpServers` snippet with a `Bearer ksr_mcp_…` placeholder for a client token.
  - A **Copy** button.

## Settings · `/w/:ws/settings/:section` · `Settings.dc.html`

Opened from the **account menu**. The left sub-nav (210px) is grouped as:

- ACCOUNT: Profile & security, API tokens
- WORKSPACE: Workspaces, Members & roles
- DEPLOYMENT: Cloudflare & keys

The page title is the section name.

- **Profile & security**
  - Profile card: avatar initials, Name, Email, Theme segmented control (Light / Dark / System, applied instantly), Time zone.
  - Security card: rows for Password ("Last changed …" · Set · **Change**), Two-step verification (On · **Manage**) and Single sign-on (Available · **Link account**).
  - "ACTIVE SESSIONS": device, location and time, with **Sign out** per session, and "Current" for this one.
- **API tokens:** table with Name and prefix (mono), Scopes (mono), Last used, Expires and **Revoke**. **Create token** opens a dialog for name, scopes and expiry, then shows the token **once** with a copy button.
- **Workspaces**
  - Table: letter tile, name and sub-line (Current · created … / Sandbox note), Your role, Members, Workflows, Run retention, and an action (Open / Switch). **Create workspace**.
  - Three explainer cards below:
    - **Switching:** use the sidebar switcher.
    - **Isolation:** credentials, runs and artifacts are scoped per workspace; exports exclude secrets.
    - **Sandbox workspaces:** external calls are recorded, not sent.
- **Members & roles**
  - Invite row: email input, role select and **Invite**.
  - Member rows: initials avatar, name and email, last active, and a role select.
  - Four role cards: Owner (everything, including deletes and key rotation), Admin (members, connections, publish), Editor (build, simulate, publish; use existing connections), Viewer (read, and respond to approvals).
- **Cloudflare & keys**
  - Resources card with an "All health checks passing" pill. Rows: Workers, Workflows, D1, R2, Queues, Durable Objects, each with resource names (mono) and a metric.
  - Encryption keys card: rows for each key version (Active / Retired pill), what it encrypts, and dates. Actions: **Export backup** and **Rotate key**. Rotation re-wraps credentials in the background, so show progress.
