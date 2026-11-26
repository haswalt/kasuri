# Connecting agents to Kasuri

Kasuri serves its MCP management endpoint directly from Rust/Wasm on Cloudflare Workers. No TypeScript API, separate MCP service or alternative workflow engine is involved. Open **MCP server** in your installation and copy the displayed endpoint. It has the deployment's origin and a workspace-specific route:

```text
https://kasuri.example/mcp/<workspace-id>
```

The routing ID is not a password. Every MCP request needs its own scoped bearer credential. Browser cookies, Basic authentication passwords and MCP session identifiers do not grant agent access. Different workspace routes and deployment origins have separate audiences; a token issued for one is rejected at another. No credentials or personal deployment details are shipped with the repository.

## Bearer credentials

Choose **Create credential**, give the client a name, select permissions and an expiry, and copy the token into the client's secure credential storage. Tokens are shown once, stored only as SHA-256 digests, expire after 1–90 days and can be revoked from the portal. Rotate by issuing a replacement, updating the client and revoking the old token.

Configure a Streamable HTTP client with the copied URL and an `Authorization: Bearer <token>` header. Client configuration formats vary; use the client's secure environment/secret integration rather than putting a credential into a repository.

| Portal profile | Granted scopes | Capabilities |
| --- | --- | --- |
| Reader | `read audit` | Connector schemas, workflows, recorded runs and audit reads |
| Builder | Reader + `write simulate` | Create/edit/clone drafts and run offline simulations |
| Operator | Builder + `publish execute control` | Activate triggers, start real runs, pause/resume/cancel runs |
| Manager | All listed scopes | Full workflow, run and data management |
| Data viewer | Reader + `data_read` | Inspect workspace datasets and records |
| Data editor | Data viewer + `data_write` | Create/rename datasets and insert, patch, replace or permanently delete records |

Credential administration is owner-only through the browser portal; agents cannot mint credentials, approve OAuth grants or change their own permissions. Scopes apply to the entire selected workspace, not individual workflows. The underlying credential API also accepts explicit subsets of the advertised scopes; MCP tools require baseline `read` plus the appropriate operation scope.

## OAuth clients and Cloudflare MCP portals

A compatible client can connect using only the copied MCP endpoint and the authorization-code flow:

1. The endpoint returns `401` with `WWW-Authenticate` pointing at this workspace's RFC 9728 protected-resource metadata.
2. That metadata identifies its authorization server. RFC 8414 discovery advertises authorization, token, registration and revocation endpoints.
3. Public clients register their exact callback URI using dynamic client registration (RFC 7591), then send an authorization request with PKCE `S256`, state and the target `resource` (RFC 8707). Omitting scopes requests the Reader defaults.
4. Kasuri opens the workspace's MCP portal. Sign in as the workspace owner, inspect the client ID and callback, and approve the requested permissions. Client names are self-reported. Read and audit permissions are selected initially.
5. A short-lived, one-use code returns to the registered callback. The client exchanges it with the matching verifier, client ID, callback and resource. Access tokens last one hour; rotating refresh access lasts up to 30 days. Reuse of a rotated refresh token revokes its grant. Clients should serialize refresh attempts.

The owner can revoke the grant from **Agent credentials**. HTTPS callbacks and HTTP loopback callbacks are supported; wildcard callbacks, fragments and callback userinfo are rejected. Authorization requests expire after ten minutes; approved codes expire after two minutes. This implementation supports public PKCE clients with `token_endpoint_auth_method=none`; it does not fetch arbitrary client-ID metadata documents or implement external identity-provider sign-in.

To use a **Cloudflare MCP server portal**, add this endpoint as an upstream server with **OAuth** authentication, complete the owner consent flow and select the tools you want exposed. Cloudflare Access's authentication to the portal is a separate layer from Kasuri's upstream OAuth grant. If you protect the deployment using Access or another external gateway, ensure it permits standard MCP/OAuth discovery and token traffic to reach these independently authenticated routes while keeping the browser/API owner surface protected. Do not configure an unauthenticated Kasuri upstream.

Kasuri implements Streamable HTTP with the `2025-11-25` initialization protocol, accepting `2025-06-18` and `2025-03-26` as well. It returns JSON responses, accepts initialized/cancellation notifications, and returns `405` for an optional GET SSE stream; there are no server-initiated subscriptions or protocol sessions. Cloudflare portals support the 2025 protocol and automatic legacy negotiation. Modern-only `2026-07-28` clients need a compatible client/portal adapter; that newer protocol is not advertised here. Portal transport compatibility follows the published documentation; a real Cloudflare portal tenant has not been connected by the local test suite.

## Tools and version checks

Discovery only lists tools permitted by the credential. The server checks scopes again on every invocation, even if a client manually calls a hidden tool.

| Scope | Tools |
| --- | --- |
| `read` | `connectors_list`, `workflows_list`, `workflows_get`, `workflows_revisions`, `workflows_trash`, `workflows_validate`, `runs_list`, `runs_get` |
| `audit` | `audit_list` |
| `write` | `workflows_create`, `workflows_update`, `workflows_clone`, `workflows_restore_revision` |
| `simulate` | `runs_simulate` |
| `publish` | `workflows_publish`, `workflows_pause`, `workflows_resume` |
| `execute` | `runs_start` |
| `control` | `runs_pause`, `runs_resume`, `runs_cancel` |
| `delete` | `workflows_archive`, `workflows_restore` |
| `data_read` | `data_tables_list`, `data_table_get`, `data_get`, `data_query` |
| `data_write` | `data_apply`, `data_batch` |

Use `workflows_get` to retrieve the saved graph and its `version`. This edit version changes on saved mutations. `revision` is a monotonic publication counter. Unchanged publish and repeated pause/resume are no-ops and preserve the edit version. Publishing unchanged paused content does not resume triggers. Update passes a `workflow` object and `expectedVersion`. Publication and execution pass `workflowId` and `expectedVersion`. Stale versions return a structured conflict. The UI also submits edit versions, so an agent edit cannot be silently overwritten by a stale UI save. The [HTTP API v2 migration](http-api.md) rejects omitted REST edit preconditions with 428 and stale versions with 409. MCP already requires explicit edit versions.

All mutating MCP tools require an 8–128 character `idempotencyKey`. Retry the exact same request with the same key after a timeout/reconnect: the original result is returned without creating another workflow/run. Reusing a key with changed arguments produces a conflict. Receipts are tied to the credential, persist across restarts and last 24 hours. Use a fresh key for a new operation or corrected request. Once that window expires, inspect current state before retrying an uncertain old operation. Mutation results contain stable IDs/status/version and identify the detail tool to call next.

`workflows_validate` does not save, publish or call external services. Draft writes do not publish. `runs_simulate` uses the same offline engine behavior as the UI; external nodes need explicit simulation fixtures. `runs_start` can perform real external writes. Publishing/resuming may activate schedules and webhook triggers. Run cancellation stops future work and cannot undo an external effect already committed. A cancelled HTTP request is not a run-cancellation command; use `runs_cancel` explicitly.

Named secret references can appear in authorized graph inspection, but secret values are never resolved by MCP discovery or reads. Common inline credential fields are redacted, including JSON-encoded headers. The API rejects saving redaction placeholders through workflow updates; use named secret aliases or explicitly replace those fields. Existing host policy still controls whether shared installation HTTP secrets may be used. MCP exposes workflow/node configuration, not arbitrary environment variables, host credentials, deployment settings or a general secret-reading tool.

## Auditing and limits

The **Audit log** page and `audit_list` return semantic workflow, credential and consent changes. Each workflow mutation records one event with actor/credential ID, a named target, outcome, affected fields/nodes and before/after state references. The dashboard opens a detail view and shows newest records first. Routine reads, execution events, token refreshes, idempotent replays and no-op saves/publications are excluded; run activity remains in the runs view. Raw arguments, configuration values, tokens and payloads are excluded from audit details. Older transport-only workflow evidence is retained with limited details; historical details are never invented. There are no agent-facing audit mutation tools. Workspace create/rename events belong to the owner catalogue and appear in the Personal workspace audit.

The current log retains up to 1,000 records within the workspace storage budget and reports its oldest retained ID and total recorded count. Consumers must report gaps when old records have aged out. `audit_list` uses an `after` cursor; workflow/run lists use bounded offset cursors and return at most 100 records per page. Runs remain subject to the existing retained-history and execution budgets. Live pagination can change while other actors mutate the workspace.

Credentials are limited to 64, OAuth client registrations to 128, pending authorization requests to 64, and unexpired mutation receipts to 64 per workspace. A registration that never completes an authorization expires after one hour, and when the registry is full the oldest such registration is replaced, so unauthenticated registrations cannot lock out new clients. Clients that have completed an authorization keep their registration for 90 days, or while they hold a grant. When pending requests are full, the oldest request still awaiting consent is dropped; approved requests are kept. Unused OAuth registrations can also be cleared from the owner portal without revoking existing grants. Public registration, OAuth and MCP requests have per-minute budgets; authenticated credentials also allow at most 120 requests per minute. Cloudflare's connecting-IP header partitions the public budgets. Requests without it use a shared bucket. These counters never authorize a request. Requests and results are bounded, and all MCP credentials/receipts share the workspace checkpoint budget.

Detailed execution logs/timings, step-input history/replay, a connection vault, per-workflow grants and MCP resources remain future work. `workflows_revisions` browses retained immutable definitions, with marked legacy baselines; `workflows_restore_revision` restores one into the draft without changing the live definition. Publish separately to roll back as a new revision. OAuth here authorizes the existing browser workspace; it does not add user accounts or team membership.

## Deployment and tests

The Worker routes `/mcp/*`, `/oauth/*` and `/.well-known/*` to Rust before static assets. Cloudflare uses the request's HTTPS origin by default. Set `PUBLIC_ORIGIN` to override the canonical origin. Do not derive trusted origins from arbitrary forwarded headers. Browser Origin headers must match the configured canonical origin; server-to-server clients normally do not send Origin.

Back up the complete Durable Object storage, including the account owner record, workspace directory and workspace checkpoint. For legacy browser-owned workspaces, also back up the session-auth row and retain the documented owner mapping. Restore to the original MCP origin to preserve the credential audience.

`pnpm test:rust` covers authorization policy, version conflicts, idempotent recovery, PKCE/redirect/audience checks, refresh replay, redaction and retention. `pnpm test:runtime` exercises the real Rust/Wasm HTTP boundary, restart persistence, workspace isolation, discovery, scopes, consent/CSRF, code exchange and revocation. No public credentials, live client grants or external identity-provider accounts are required for these tests.

Protocol references: [MCP 2025 transport](https://modelcontextprotocol.io/specification/2025-11-25/basic/transports), [MCP authorization](https://modelcontextprotocol.io/specification/2025-11-25/basic/authorization), [modern/legacy compatibility](https://modelcontextprotocol.io/specification/2026-07-28/basic/versioning), [Cloudflare MCP portals](https://developers.cloudflare.com/cloudflare-one/access-controls/ai-controls/mcp-portals/).

Draft saves preserve the published graph, webhook authentication and schedule. `lifecycle.publishedRevision`, `lifecycle.enabled` and `hasUnpublishedChanges` describe these independent states. History starts with this storage version; legacy active/paused definitions become explicitly marked current baselines, while edited legacy drafts with unavailable live definitions require explicit review and publication. History currently shares the bounded workspace checkpoint budget.

Workflow trash is recoverable. Archiving stops future triggers and scheduling, retains definitions/history/runs, and leaves in-flight runs to finish. Restoring never reactivates triggers. The `delete` scope is explicit; existing Operator/Builder grants are not expanded. No permanent-delete tool is exposed.

The earlier `/mcp/<workspace-id>/mcp` URL and its protected-resource discovery URL remain compatibility aliases. Existing bearer/OAuth grants stay bound to their original audience, and existing OAuth clients may refresh against that same audience through the unchanged issuer. New portal credentials and discovery use the canonical single-`mcp` path. No credential-bearing POST is redirected, and tokens are not silently accepted across both audience URLs.


## Dataset management

Dataset scopes are explicit opt-ins. Existing Reader/Builder/Operator credentials and previously issued Manager grants are not expanded when dataset tools are installed. New Data viewer/Data editor profiles grant dataset access, and newly issued Manager credentials include the displayed dataset scopes. Baseline `read` is still required for every tool. Data editor permissions do not grant workflow draft editing or execution.

Use `data_tables_list` for metadata summaries and `data_table_get` for schema/version details. `data_get` reads a key; `data_query` accepts bounded JSON-pointer filters and keyset cursors. An empty query page may still have `nextCursor`. Input discovery includes the supported command variants and query schema.

`data_apply` accepts one mutating `command`; `data_batch` accepts up to 100 mutating `commands`. Both require `idempotencyKey`. Prefer `patch` with an observed row version when changing known fields, so unmentioned fields and expiry remain intact. Credential-shaped values are redacted in read results; writing redaction placeholders is rejected. Replace them deliberately or patch only the fields you know. Data mutations return compact status/version references rather than copying record values into transport receipts; use a read tool for details.

The SQLite host commits data mutations, durable data receipts and semantic audit together. MCP transport receipts provide an additional replay layer. A retry after a lost transport receipt still recovers the stored effect without repeating its mutation or audit. Receipt identities are credential-scoped and separate from owner API/engine domains. The server reserves bounded receipt space before executing a data mutation.

Record deletion is permanent and is explicitly part of `data_write`; it does not reuse the workflow-only `delete` scope. Table deletion/schema migration remain separate implementation increments.

Data workflow execution also requires explicit dataset scopes: simulation needs `data_read`, while starting/resuming live mutations or publishing/resuming their triggers additionally needs `data_write`. The ordinary workflow execution/publication scopes still apply. `runs_get` requires `data_read` for a run containing data nodes because its outputs can contain dataset values. Run controls inspect the pinned run graph; trigger resumption inspects the published graph. Existing workflow grants cannot bypass dataset restrictions by dispatching a data node.

## Building workflows without the visual editor

Start with these read-scoped discovery tools; they never install examples or mutate workspace data:

| Tool | Returns |
| --- | --- |
| `workflows_schema_get` | Versioned JSON Schema for the graph, nodes, edges, retry/position/default fields and implemented connector configuration. |
| `platform_capabilities_get` | Data flow, branch/child semantics, publication, simulation, retries, scopes and limits. |
| `workflows_examples_list` | Paginated metadata for the repository's tested synthetic workflows, including dependency IDs. |
| `workflows_examples_get` | One complete workflow, its synthetic input, asserted outputs and dependency/publication instructions. |
| `connectors_list` | Implemented names/versions/config schemas/ports/side effects, descriptions, input ports and configurations taken from public examples. |
| `connectors_get` | One connector's input/output schemas, optionally specialized for supplied config, selected array rules, data-flow explanation and public usage examples. |

The schema is generated from the engine's Rust/Serde graph types and composed with the actual connector manifests. Create/update/validate tool input schemas embed this same contract with resolvable local definitions. Runtime publication/execution uses the same structural/configuration schema plus topology and adapter-specific checks. The platform API also exposes `GET /api/workflows/schema` and `POST /api/workflows/validate` (the latter accepts the workflow object directly). These routes require platform access, not an MCP token.

Canonical field names are `nodes[].type` for the connector name, required `nodes[].label` for the display name, and `edges[].source`, `target`, `sourcePort` for connections. Node and edge IDs are stable, workflow-local references. Version defaults to 1, sourcePort to `success`, timeoutMs to 30000 and retry.maxAttempts to 1. There is no `connectorKind`, `from`, `to` or `targetPort` graph field. IF ports are `true`/`false`/`error`; Switch ports are configured rule names plus `fallback`/`error`. Other nodes use `success`/`error`.

Use this loop:

1. Discover the schema, capabilities and relevant connector examples.
2. Construct a definition or fetch a public example. Change IDs and references consistently when avoiding existing workflow IDs.
3. Call `workflows_validate` with `{"workflow": ...}`. This checks shape, topology, connector configuration and graph-local contracts without saving or resolving live publication availability.
4. Fix reported paths and validate again. Do not retry unchanged invalid definitions.
5. Call `workflows_create` with the workflow and a fresh idempotencyKey; obtain the edit version with `workflows_get`.
6. Simulate using that expectedVersion, then inspect the returned run using `runs_get`. Simulations may be asynchronous; poll until terminal.

Validation returns bounded, structured diagnostics, for example:

```json
{
  "valid": false,
  "schemaVersion": 1,
  "errors": [
    {
      "code": "REQUIRED_PROPERTY",
      "path": "/nodes/0/label",
      "message": "Required property is missing",
      "nodeId": "start",
      "connector": "Manual Trigger"
    }
  ],
  "errorsTruncated": false
}
```

Codes cover missing/wrong-typed fields, unsupported schema/connector values, duplicate IDs/connections, missing edge endpoints, invalid output ports, unreachable/cyclic graphs, retry/deadline limits and connector configurations. `expected` supplies supported constants, enums or JSON types where available; `suggestion` is optional. Paths are JSON Pointers relative to the proposed workflow. Diagnostics omit raw input values and upstream error bodies. Later validation stages may require another pass after structural errors are corrected. A maximum of 32 errors is returned; errorsTruncated is conservatively true at that bound.

For a parent/child example, fetch `19-invoke-workflow`; its dependency is `18-reusable-workflow`. Create the child using its declared ID, publish it with explicit publish scope, then create/simulate the caller. If choosing fresh IDs, update the caller's workflowId references. Validation alone does not publish dependencies or verify their deployment-local existence. Workflow-to-workflow and Each simulations still require a valid published child; data-access scopes remain explicit.

Discovery assets are compiled from public repository fixtures, not production workflows. Their schema properties must remain intact even when named password/token/authorization; workspace/run responses still receive credential redaction. Existing scoped access, expectedVersion and idempotency requirements are unchanged. Higher-level add-node/connect-node mutation tools remain future work; complete-document building is supported through this contract now.

Workflow format 2 adds optional `inputBindings` on non-trigger nodes: named expressions, using the same operations as the Expressions connector plus `workflow` (`/id`, `/name`, `/revision`), that build the node's input object. `platform_capabilities_get` describes the rules, and `workflows_schema_get` includes the binding schema. A connector's `inputSchema` from `connectors_get` then describes the assembled object.

Runtime contract inspection is pure and read-scoped: for example `connectors_get` with `{"kind":"Aggregate","config":{"operation":"sum","arrayPointer":"/items"}}` explains that the selected input is an array at `/items` and the output is a number itself. HTTP text responses expose a string at `/body`; awaited child results are at `/output`. An `Execute Workflow Trigger` reports its declared `inputSchema` as both input and output, and its declared result contract under `childContract`. Configurations that fail at runtime, such as `JSON Utilities` without an `operation`, return open schemas instead of a guessed contract. Open schemas deliberately represent input-dependent values rather than inventing guarantees. Supplied mapping values are used only to infer types, never returned as sample data; workspace secrets and runs are not inspected.

## Confirm the connected workspace

Call `workspace_identity_get` on any authenticated MCP connection. Compare its `workspaceId` with the ID displayed on the dashboard MCP server page. The response identifies the authenticated credential and its scopes. New credentials also show who issued or approved them in `authorizedBy`. Earlier grants may have no recorded user. See [platform sign-in and workspace recovery](platform-auth.md).
