# Internal platform implementation plan

Status: implementation in progress; completed increments are recorded below.
Date: 18 November 2026.

This plan combines the reported product issues with the supplied n8n migration gap analysis, checked against the current source. The attachment is requirements/reference material; its suggested connector priorities do not override the request to exclude external connectors. No source workflow exports or production datasets were supplied, so this is not a claim of verified n8n migration parity. The original attachment is not copied into the public repository.

## Implementation progress

- Completed: body-portaled, viewport-positioned action menus (`4be2bbe`).
- Completed: combined Built in/Planned, category and search filters persisted in catalogue URLs (`571fce6`).
- Completed: selected connection inspector, endpoint/port edits, explicit deletion and highlighted reconnect handles (`b1d327c`). Browser verified; disconnected drafts remain editable and fail executable validation.
- Completed: browser ownership directory with workspace creation, rename and per-tab selection; isolated workspace storage/credentials/endpoints; existing Personal workspace preserved. Runtime checks cover forged delegation headers, cross-session access and restart persistence. Shared-user memberships remain future account work.
- Next: separate record storage and publication/history/lifecycle foundations. Workflow deletion and remaining management audit changes are still pending.

## Outcomes and boundaries

The goal is a composable internal workflow platform: reliable draft/publication state, editable graphs, persistent workspace data, flexible requests/responses, collection processing and reusable workflows. Business rules such as duplicate-event handling belong in workflows. The Rust engine remains responsible for durable execution, scheduling, checkpoints and safe effect dispatch.

Keep the React/Panda UI, Rust service/connector crates and Rust/Wasm entrypoint. Each capability must work on Cloudflare and self-hosted workerd. Do not add a TypeScript API, provider-dependent execution engine, seeded installation data or an application demo mode. Maintain importable examples and tests for every supported internal connector.

In scope: all reported product issues; Data Tables/context storage; expressions; bounded Code; collections; parallel branches; workflow invocation; webhook responses; core scheduling, artifact and credential infrastructure. Out of scope: Google Calendar, Twilio, Resend, Anthropic, model/agent adapters, ElevenLabs, Voiceflow and other external service connectors. Generic HTTP/file formats and storage/credential abstractions are platform work; adding provider-specific workflow adapters remains separate.

## Confirmed starting points

- Canvas code already contains edge deletion and reconnect handlers, but selection is node-centric, there is no edge inspector and controlled graph updates do not preserve an explicit selected-edge model. Existing plumbing is not a usable connection-management feature.
- Workflow deletion is explicitly disabled. Menus render inside their trigger container, including table overflow containers; increasing z-index cannot solve that clipping.
- The connector catalogue has categories/search, but no implementation-status filter.
- The service stores one mutable workflow document. Saving sets it to draft and clears its next schedule. Publication increments a counter; it does not retain an immutable published graph or check whether content changed. Runs pin graph snapshots, but these are not a trustworthy publication-history database.
- Browser routing is hard-coded to `personal`; the cookie selects one workspace. Real creation/switching requires identity/membership routing, not another dropdown entry.
- Management audit currently records generic API/tool actions and some run events. It lacks a useful change-detail model and can record the same operation at multiple layers.
- Webhooks require authentication and an event ID, accept JSON POSTs and apply host-side business-event deduplication. MCP and webhook URLs expose internal route structure.
- Workspace persistence is currently a bounded serialized checkpoint in a SQLite row. Dataset growth, revision history and larger artifacts must not simply be added to that JSON blob.

Relevant implementation areas: `apps/web/src/components/WorkflowCanvas.tsx`, `WorkflowEditor.tsx`, `design/ui.tsx`, `App.tsx`, `lib/navigation.ts`; `crates/workflow-service`, `workflow-engine`, `workflow-worker`, `workflow-mcp`, `connectors-webhook` and `workflow-schema`.

## Required behavior decisions

### Draft, live revision and activation are independent

Replace the overloaded status with explicit state:

- **Draft:** the editable graph and its optimistic-concurrency edit version.
- **Published revisions:** immutable, numbered definition snapshots.
- **Published pointer:** the revision selected for new production executions.
- **Enabled:** whether production triggers accept new events.
- **Has unpublished changes:** comparison of the draft's publishable content with the current published snapshot.
- **Archived:** recoverable workflow deletion, independent of historical runs and revisions.

| Action | Required result |
| --- | --- |
| Create/save an unpublished workflow | Persist a draft; do not activate triggers. |
| Save changes to an active workflow | Update the draft only. The existing published graph, auth policy, schedule and enabled state remain live. Show **Active · unpublished changes**. |
| Save changes to a paused workflow | Update the draft; keep the published graph paused. |
| Publish a changed, valid draft | Atomically create the next immutable revision, update the published pointer and activate its bindings. Label this action **Publish & activate** where activation would change. |
| Publish unchanged content | Disable the UI action and make the API a no-op. No new revision, trigger registration or audit-change record; preserve active/paused state. |
| Pause/resume | Change activation only. Resume the existing published revision, never an unpublished draft, and do not create another revision. |
| Restore a previous revision | Create a draft from that snapshot, show its diff, then publish it as a new monotonic revision. Never rewrite old history or running checkpoints. |

Canonical publication comparison must ignore server timestamps, counters and JSON object ordering. Use stable IDs and normalized graph ordering. Recommended default: canvas-only positioning changes are saved editor metadata and do not produce a new execution revision. Define and test the publishable-content boundary once in the Rust service, not separately in the UI and MCP.

Production webhooks, schedules and child invocations use a published snapshot. Editor **Run draft** remains available for an explicit real test, preserving the existing import-and-run experience; it must identify the draft/edit version and warn about real external effects. Dashboard production runs use the published revision. Simulation can target a saved draft or historical revision and clearly identifies which definition was used.

### Public ingress is distinct from the platform API

Use stable, deployment-local workspace public keys, separate from internal Durable Object IDs and mutable display names:

```text
/api/...                              platform management API
/mcp/<workspace-key>                  agent management endpoint
/hooks/<workspace-key>/<hook-key>     workflow trigger endpoint
```

The public key is routing information, not authentication. Keys remain stable on workspace rename. Hook keys can have validated friendly paths with uniqueness checks. Keep existing MCP/webhook routes as explicit compatibility aliases while migrating; do not redirect credential-bearing MCP POSTs or silently change existing OAuth token audiences. Newly issued credentials/discovery use the new canonical MCP URL. Existing grants either continue against their original alias/audience or are explicitly reauthorized.

### Webhook authentication and duplicate handling are separate

- New webhook triggers default to **no authentication**. Header token, secret query parameter and HMAC/signature verification are explicit options. A selected auth mode can naturally require its own authentication material; there is no unconditional event-ID requirement.
- Preserve existing endpoints' configured protection on upgrade. Making a previously protected endpoint public requires an explicit published change.
- Remove mandatory `X-Kasuri-Event-Id` and automatic business-event deduplication from normal webhook dispatch. Do not discard repeated payloads merely because they look alike.
- Expose permitted method/path/query/header/body metadata through request context and expressions. Remove consumed authentication secrets from recorded context, redact configured sensitive fields, and do not log raw secret-bearing URLs.
- Build duplicate handling with a user-selected key and an atomic Data Tables/context operation. A lookup followed by a separate insert is insufficient under concurrent requests.
- Preserve **engine effect idempotency**: recovering a persisted node task must not repeat its own already-recorded table mutation or child invocation. This is runtime retry safety, not a decision that two incoming business events are duplicates.

A deduplication claim must complete before the protected side effect. Running the claim in parallel with that side effect cannot guarantee deduplication. Parallel branches are useful for independent context updates, enrichment and reporting after required gates have passed.

### Audit is management change history

Audit should explain what changed in the dashboard/platform, whether the actor was a person or an agent:

- Workflow creation, meaningful draft edits, publication, rollback, activation and archive/restore.
- Workspace creation/settings/membership changes; credential and connection administration; table schema and manual management changes.
- No run starts, pauses, completions, retries, node dispatches or workflow-driven table writes in this log. These belong in execution history/trace, including when an MCP client starts the run.
- No entries for ordinary reads, refreshes, unchanged saves/publications or idempotent replays. Emit one authoritative semantic event per operation, not a generic PUT plus another MCP tool record.

An entry needs a readable action, actor name/type and stable ID, workspace, resource type/ID/name, timestamp, change summary, correlation ID and before/after references. Clicking it opens a detail drawer with field/node/edge changes, version links and related management context. Preserve labels for deleted resources. Redact secrets before storing audit details; table-management summaries must not leak private row contents to audit-only readers. Report retention gaps honestly and retain the existing records as a clearly identified legacy category, rather than presenting them as richer change records.

## Delivery sequence

Each numbered delivery has its own reviewable Conventional Commit(s), tests and updated documentation. Implement these in order unless a stated spike can be investigated independently. No additional SaaS connectors are needed to demonstrate any milestone.

### 1. Editor and library usability — immediate fixes

1. Introduce a controlled selection union for node, edge or none. Edge click selects/highlights that edge and opens a sidebar showing source node/output port, destination node/input and connection ID. Provide explicit **Delete connection**, Delete/Backspace and accessible keyboard selection. Deleting an edge keeps its nodes and does not silently reconnect them.
2. Make endpoint reconnection handles visible and usable by dragging either end. Reuse server/shared validation for self-edges, cycles, invalid ports, duplicate connections and incoming trigger edges. Keep the edge ID on reconnection. Provide a sidebar alternative to dragging.
3. Permit disconnected/incomplete graphs as drafts, with visible validation errors. Publishing/executing still requires an executable graph. Do not make an edge impossible to remove because the resulting draft is temporarily invalid.
4. Move menus into a top-layer/body portal with anchored positioning, flip/shift and scroll/resize updates. Update outside-click handling to include the portaled menu, and preserve focus restoration and keyboard navigation. Test the bottom/right rows of a scrolling table; z-index alone is not a fix.
5. Add **All / Built-in / Planned** connector filtering, combined with category and search, reflected in result counts and URL state.

Acceptance: selecting/deleting/reconnecting edges survives a save/reload; invalid reconnections leave the graph unchanged; context menus remain usable outside scroll bounds at supported desktop widths; all combinations of connector filters are correct. Persistent publish history is a later delivery, not a substitute implemented only as local undo.

### 2. Real workspaces and storage foundations

- Separate the browser's owner/session identity from its selected workspace. Add a membership-aware workspace catalogue, create/list/rename/switch APIs and the workspace switcher/create dialog. Initial scope is multiple workspaces for the existing owner session; do not pretend this also implements multi-user accounts.
- Put workspace identity in the route/request context so two browser tabs can operate on different workspaces. A mutable global active-workspace cookie must not retarget requests from another tab. Verify membership server-side; route IDs and caller-supplied headers are not permission grants.
- Migrate the existing verified workspace session into the catalogue without losing its workflows, grants, endpoints or runs. Preserve the existing workspace's underlying identity.
- Introduce versioned storage migrations and separate persisted records for drafts/revisions, execution checkpoints, management audit, grants/receipts and datasets. Keep large datasets outside run checkpoints. Use the same logical Rust storage/effect interface for Cloudflare and workerd.
- Add stable public routing keys and registry lookup needed by the simpler URLs. Plan compatibility aliases and backup/restore before changing public endpoints.
- Start the bounded JavaScript-runtime feasibility spike here, so it does not become a late surprise in delivery 7.

Acceptance: two workspaces can be created and used in parallel; workflow/table/artifact/credential/MCP/webhook access cannot cross memberships; existing session migration is restart-safe; renaming a workspace does not invalidate its stable ingress keys.

### 3. Publication lifecycle, history, deletion and management audit

- Implement the independent draft/published/enabled/archived model and canonical content fingerprint described above. Atomically update publication snapshots and trigger registrations, with edit-version checks across UI, REST and MCP.
- Retire the compatibility bypass for unversioned existing-workflow REST writes through an explicit API migration. Missing edit preconditions must be rejected once that migration is complete; another client must not be able to bypass concurrency control by omitting the version. New workflow creation remains distinct from updating an existing one.
- Add editor history showing immutable revisions, who published them, timestamps and diffs. Implement restore-to-draft and explicit rollback publication. Keep in-flight runs pinned to the definition with which they started.
- Implement recoverable workflow deletion as archive/trash. Disable future ingress and scheduling atomically, hide archived workflows from normal lists, retain history and runs, and support restore without reactivation. Existing runs finish by default; cancellation is an explicit separately authorized choice. Check references before archiving a child workflow.
- Build the contextual audit detail model and UI above. Move execution events to run trace and remove duplicate transport/service audit writes. Integrate semantic events with state changes in the same persistence boundary.
- Update list badges, filters, editor controls, run definition labels, webhook status, scheduling and MCP tools to use the same server state. Introduce a distinct destructive-management permission rather than silently expanding existing editor grants.

Acceptance: unchanged publish does nothing; paused unchanged publish does not resume; active draft saves do not change the live graph/auth/schedule; paused saves remain paused; rollback produces a new revision; archive blocks new triggers while retaining historical evidence; audit detail explains an actual change exactly once and excludes all run activity.

### 4. Data Tables and persistent context

- Add workspace-scoped table/field/index management and insert/get/query/update/upsert/delete/bulk operations, with filtering, ordering and bounded cursor pagination. Provide a dashboard table view/editor and JSON/CSV import/export for synthetic or user-provided data.
- Add a **Data Tables** connector and a small **Context Store** interface/preset over the same primitives for key/value get/set/delete, TTL and invalidation. They share persistence and permission policy; do not build competing state engines.
- Support unique keys, conditional update/version checks and atomic `insert if absent`/compare-and-set. Return structured outcomes such as `inserted`, `existing` and `conflict` that a normal IF node can consume.
- Store task-effect receipts with mutations so a crash after committing a table write but before completing a checkpoint cannot repeat an increment or bulk write. Scope these receipts to run/node/item identity.
- Use validated query structures and bound SQL parameters; no arbitrary SQL connector. Enforce data permissions and expose new data-management MCP scopes explicitly, without upgrading old grants automatically.
- In simulation, use explicit fixture data or a permitted read snapshot plus a copy-on-write overlay. Never commit simulated writes or TTL changes to live tables.

Acceptance: concurrent requests claiming the same key produce one winner; retries/restarts do not double-apply writes; TTL reads exclude expired rows; pagination is bounded; data is workspace-isolated; simulation leaves the live database unchanged. Ship examples for a persistent counter, cache, data API and user-selected event deduplication.

### 5. Flexible webhook ingress, request context and responses

- Move canonical triggers to `/hooks/...` and implement configurable paths/methods, JSON/text/form-urlencoded inputs, query parameters, permitted headers and configured CORS. Use the published snapshot for route/auth/parser configuration.
- Implement optional auth and remove mandatory event IDs/host business deduplication, with migration behavior described above. Preserve legacy HMAC compatibility explicitly while introducing signature schemes that do not require an unrelated business event ID.
- Add **Request Context** to read immutable trigger metadata. Keep it distinct from **Context Store**, which persists information across runs. Expressions and downstream nodes can read request context without inserting headers into every payload.
- Offer asynchronous acknowledgement and a bounded synchronous response mode. Add **Respond to Webhook** with validated status, headers and JSON/text body. Persist the response decision once; define behavior for no response, late response, failure, duplicate response attempts and a disconnected client.
- A response timeout/disconnection must not silently cancel a durable run. Set an explicit response-window policy; after the window closes, record delivery state in run trace and do not attempt a second HTTP response. Keep platform session cookies and hop-by-hop headers out of workflow-controlled response headers.
- Implement optional deduplication examples using Request Context → atomic table claim → IF → business action/response. Include a no-auth example that needs no special headers and a query-secret example with redacted request traces.

Acceptance: a plain request without auth/event-ID headers triggers an unauthenticated configured endpoint; header/query/HMAC modes work only when selected; repeated requests are distinct unless the workflow claims a key; custom 200/201/400/409 responses return the configured body; CORS and timeout behavior are consistent on Cloudflare and workerd.

### 6. Durable branch, parallel and item execution

- Add explicit **Split / Parallel** and **Join** semantics. Existing multi-edge fan-out is not evidence of concurrent execution: the current host has one external in-flight slot per run. Generalize it to a persisted set of tasks with bounded concurrency and completion correlation.
- Persist every task before dispatch. Merge each completion into the latest checkpoint so cancellations and unrelated branch completions cannot overwrite one another. Track all branches; do not use unrecorded fire-and-forget promises.
- Specify join policies (wait for all, collect results, fail/cancel remaining, continue with explicit branch errors), ordering and branch context. Include context writes in a parallel example, while keeping correctness gates before protected effects.
- Add stable item identities, per-item context, Each/Loop, Split Items/Batch and Aggregate operations. Define all-items versus each-item execution and per-item retry/failure policies.
- Preserve schema-v1 single-JSON behavior. An existing array-valued output must not silently become multiple executions. Introduce explicit adapters/new schema semantics and migrate examples deliberately. Expand Merge with append/by-key/selected-branch policies.
- Use one due-work scheduler for retries, waits, child completions, response deadlines and TTL maintenance, so one capability cannot overwrite another's alarm.

Acceptance: independent branches overlap in real execution; slow context work does not serialize an explicitly independent branch; joins and output ordering are deterministic; restart/cancel during multiple in-flight tasks is safe; per-item retries do not rerun successful items or duplicate effects. Demonstrate parallel work with deterministic local fixtures, not an external provider dependency.

### 7. Expressions, conditions, routing and bounded Code

- Define a shared execution context with current value/item, approved prior-node outputs addressed by stable node ID, trigger/request metadata, run identity and execution time. Prior-output references must respect graph dependencies and explicitly handle skipped steps.
- Add typed interpolation, arithmetic, string/date operations, existence/null checks, numeric/string/date comparisons and AND/OR groups. Keep expression parsing/evaluation bounded and distinguish missing values from null. Use the same implementation in validation, runs and simulation.
- Add **Switch / Router**, richer Merge behavior, explicit empty outputs and configurable continue-on-error/branch error routing.
- Deliver a JavaScript-compatible **Code** node only after the early runtime spike proves enforceable memory, instruction/CPU, recursion, output and regex/string bounds on both native Rust and wasm32. Evaluate candidate interpreters against those requirements; do not choose one based only on native support or add host `eval`.
- The sandbox receives JSON/context only, no environment, host objects, secret store, filesystem or implicit network. Network access remains connector work. Inject a controlled clock and deterministic simulation behavior. Define single-value versus multiple-item returns and structured code failures.

Acceptance: representative scoring, date/slot calculation, validation and array transformations work from synthetic fixtures; forbidden host access fails; infinite loops, memory growth and pathological expressions terminate within policy; simulation performs no external I/O. Full n8n expression-language compatibility is not required.

### 8. Workflow-to-workflow execution

- Add **Execute Workflow Trigger** and **Execute Workflow** with explicit input/output contracts, pinned published revision selection, synchronous await and asynchronous child-run modes.
- Invoke through internal Rust services/effects, not public HTTP endpoints or webhook credentials. Enforce workspace/resource permissions, child availability and parent/child scope policy.
- Persist parent/child correlation and a stable invocation identity. Retrying or restoring a parent task must attach to the same child rather than create another one. Propagate simulation mode into children and never invoke a real child from a simulation.
- Define error propagation, cancellation, deadlines, depth/recursion and fan-out budgets. Show child runs and selected revisions in the debugger. Archiving/changing a workflow must not rewrite an already-running child snapshot.

Acceptance: a parent invokes a reusable synthetic calculation, awaits its result or continues with a child ID, and survives restart at each boundary; repeated invocation receipts do not duplicate children; child errors and cancellation follow the chosen policy; inaccessible/unpublished targets are rejected.

### 9. Remaining internal infrastructure

- Add binary HTTP bodies/responses, multipart/form-urlencoded formats and artifact references. Keep large files/collections outside checkpoints. Introduce a storage interface with a self-hosted backend and deployment adapters; provider-specific workflow storage connectors are separate work. Enforce MIME/size/access/retention policies and cleanup across restarts.
- Build workspace-scoped reusable connection/credential administration for generic HTTP/header/Basic/OAuth use, with encrypted secret storage, rotation, testing and alias references. Existing installation-wide `HTTP_SECRETS` is compatibility infrastructure, not the completed vault. Inbound MCP OAuth and outbound provider credentials remain separate systems.
- Add interval scheduling and schedule-configuration improvements on the same published-definition and due-work infrastructure, with timezone/DST tests. Preserve existing durable waits/recovery rather than implementing another timer engine.
- Extend debugger item, branch, child and response/artifact views as their backing data becomes available. Do not fabricate timing traces or historical inputs.

Acceptance: binary data is accessed by authorized durable references rather than checkpoint embedding; credentials never enter Code globals, exports, audit patches or simulation fixtures; minute/hour intervals and timezone transitions recover correctly on both runtimes.

## Migration and verification gates

1. Back up complete storage and make migrations explicit/versioned. Existing immutable publication history cannot be reconstructed retroactively because it was never stored. Seed known current publications where reliable; flag ambiguous edited-after-publication records for review. Do not invent past revisions or silently expose a draft as the old live graph.
2. Preserve existing browser ownership, workspace identifiers, grants and endpoint aliases. Distinguish changing a display label from changing an OAuth audience. Keep existing webhook protection until an explicit published config change.
3. Pin existing runs to their stored graph snapshots. Test old checkpoint readers/migrations before changing item/parallel state shapes. Never migrate by discarding active runs.
4. Add Rust contract/service/engine tests, real HTTP/workerd tests and focused browser tests for each delivery. Exercise stale UI/MCP edits, concurrent claims, duplicate task completions, restarts, response deadlines and workspace isolation. Run the same relevant checks against Docker; keep Cloudflare asset routing and self-host ingress coverage aligned.
5. Maintain a supported-node coverage gate and importable simple/combined JSON examples for every new internal node. Fixtures must be synthetic, contain no private identifiers/secrets, and simulate offline. Include acceptance workflows for data queries/response APIs, branching/parallel context updates, optional deduplication and parent/child execution.
6. Treat the supplied twelve-workflow assessment as a capability checklist. After the internal foundations land, use sanitized actual exports and expected datasets/results to verify parity. Do not claim the external-service scenarios are migrated while their adapters and credentials remain out of scope.

Start implementation with delivery 1, then the workspace/storage and publication model. Those foundations should precede more connectors: they fix the current product behavior and give Data Tables, responses and subworkflows stable semantics to build on.
