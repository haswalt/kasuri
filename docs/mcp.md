# Connecting agents to Kasuri

Kasuri serves its MCP management endpoint directly from Rust/Wasm on Cloudflare Workers and self-hosted workerd. No TypeScript API, separate MCP service or alternative workflow engine is involved. Open **MCP server** in your installation and copy the displayed endpoint. It has the deployment's origin and a workspace-specific route:

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
| Manager | Operator + `delete` | Move workflows to recoverable trash and restore them |

Credential administration is owner-only through the browser portal; agents cannot mint credentials, approve OAuth grants or change their own permissions. Scopes apply to the entire selected workspace, not individual workflows. The underlying credential API also accepts explicit subsets of the advertised scopes; MCP tools require baseline `read` plus the appropriate operation scope.

## OAuth clients and Cloudflare MCP portals

A compatible client can connect using only the copied MCP endpoint and the authorization-code flow:

1. The endpoint returns `401` with `WWW-Authenticate` pointing at this workspace's RFC 9728 protected-resource metadata.
2. That metadata identifies its authorization server. RFC 8414 discovery advertises authorization, token, registration and revocation endpoints.
3. Public clients register their exact callback URI using dynamic client registration (RFC 7591), then send an authorization request with PKCE `S256`, state and the target `resource` (RFC 8707). Omitting scopes requests the Reader defaults.
4. Kasuri opens the owner's MCP portal. The owner must use the browser session that owns that workspace, inspect the client ID/callback and explicitly approve selected permissions. Client names are self-reported. Read/audit permissions are selected initially; writes and execution are not automatically approved.
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

Use `workflows_get` to retrieve the saved graph and its `version`. This edit version changes on saved mutations. `revision` is a monotonic publication counter. Unchanged publish and repeated pause/resume are no-ops and preserve the edit version. Publishing unchanged paused content does not resume triggers. Update passes a `workflow` object and `expectedVersion`. Publication and execution pass `workflowId` and `expectedVersion`. Stale versions return a structured conflict. The UI also submits edit versions, so an agent edit cannot be silently overwritten by a stale UI save. The [HTTP API v2 migration](http-api.md) rejects omitted REST edit preconditions with 428 and stale versions with 409. MCP already requires explicit edit versions.

All mutating MCP tools require an 8–128 character `idempotencyKey`. Retry the exact same request with the same key after a timeout/reconnect: the original result is returned without creating another workflow/run. Reusing a key with changed arguments produces a conflict. Receipts are tied to the credential, persist across restarts and last 24 hours. Use a fresh key for a new operation or corrected request. Once that window expires, inspect current state before retrying an uncertain old operation. Mutation results contain stable IDs/status/version and identify the detail tool to call next.

`workflows_validate` does not save, publish or call external services. Draft writes do not publish. `runs_simulate` uses the same offline engine behavior as the UI; external nodes need explicit simulation fixtures. `runs_start` can perform real external writes. Publishing/resuming may activate schedules and webhook triggers. Run cancellation stops future work and cannot undo an external effect already committed. A cancelled HTTP request is not a run-cancellation command; use `runs_cancel` explicitly.

Named secret references can appear in authorized graph inspection, but secret values are never resolved by MCP discovery or reads. Common inline credential fields are redacted, including JSON-encoded headers. The API rejects saving redaction placeholders through workflow updates; use named secret aliases or explicitly replace those fields. Existing host policy still controls whether shared installation HTTP secrets may be used. MCP exposes workflow/node configuration, not arbitrary environment variables, host credentials, deployment settings or a general secret-reading tool.

## Auditing and limits

The **Audit log** page and `audit_list` return semantic workflow, credential and consent changes. Each workflow mutation records one event with actor/credential ID, a named target, outcome, affected fields/nodes and before/after state references. The dashboard opens a detail view and shows newest records first. Routine reads, execution events, token refreshes, idempotent replays and no-op saves/publications are excluded; run activity remains in the runs view. Raw arguments, configuration values, tokens and payloads are excluded from audit details. Older transport-only workflow evidence is retained with limited details; historical details are never invented. There are no agent-facing audit mutation tools. Workspace create/rename events belong to the owner catalogue and appear in the Personal workspace audit.

The current log retains up to 1,000 records within the workspace storage budget and reports its oldest retained ID and total recorded count. Consumers must report gaps when old records have aged out. `audit_list` uses an `after` cursor; workflow/run lists use bounded offset cursors and return at most 100 records per page. Runs remain subject to the existing retained-history and execution budgets. Live pagination can change while other actors mutate the workspace.

Credentials are limited to 64, OAuth client registrations to 128, pending authorization requests to 64, and unexpired mutation receipts to 64 per workspace. Unused OAuth registrations can be cleared from the owner portal without revoking existing grants. Public registration, OAuth and MCP requests have per-minute budgets; authenticated credentials also allow at most 120 requests per minute. Cloudflare's connecting-IP header partitions the public budgets; self-hosting without that header uses a shared bucket. These counters never authorize a request. Requests and results are bounded, and all MCP credentials/receipts share the workspace checkpoint budget.

Detailed execution logs/timings, step-input history/replay, a connection vault, per-workflow grants and MCP resources remain future work. `workflows_revisions` browses retained immutable definitions, with marked legacy baselines; `workflows_restore_revision` restores one into the draft without changing the live definition. Publish separately to roll back as a new revision. OAuth here authorizes the existing browser workspace; it does not add user accounts or team membership.

## Deployment and tests

The Worker routes `/mcp/*`, `/oauth/*` and `/.well-known/*` to Rust before static assets. Cloudflare uses the request's HTTPS origin by default. Behind a self-hosted TLS ingress, configure `PUBLIC_ORIGIN` and route these paths to workerd; see [self-hosting](self-hosting.md). Do not derive trusted origins from arbitrary forwarded headers. Browser Origin headers must match the configured canonical origin; server-to-server clients normally do not send Origin.

Back up the complete Durable Object storage, including the browser session-auth row and workspace checkpoint. Losing the browser cookie loses owner access; copying only graphs does not transfer grants. Restore to a stable origin or reauthorize clients. The server fails closed if management state survives a partial restore without its session-auth row.

`pnpm test:rust` covers authorization policy, version conflicts, idempotent recovery, PKCE/redirect/audience checks, refresh replay, redaction and retention. `pnpm test:runtime` exercises the real Rust/Wasm HTTP boundary, restart persistence, workspace isolation, discovery, scopes, consent/CSRF, code exchange and revocation. The same MCP checks run in the Docker integration harness. No public credentials, live client grants or external identity-provider accounts are required for these tests.

Protocol references: [MCP 2025 transport](https://modelcontextprotocol.io/specification/2025-11-25/basic/transports), [MCP authorization](https://modelcontextprotocol.io/specification/2025-11-25/basic/authorization), [modern/legacy compatibility](https://modelcontextprotocol.io/specification/2026-07-28/basic/versioning), [Cloudflare MCP portals](https://developers.cloudflare.com/cloudflare-one/access-controls/ai-controls/mcp-portals/).

Draft saves preserve the published graph, webhook authentication and schedule. `lifecycle.publishedRevision`, `lifecycle.enabled` and `hasUnpublishedChanges` describe these independent states. History starts with this storage version; legacy active/paused definitions become explicitly marked current baselines, while edited legacy drafts with unavailable live definitions require explicit review and publication. History currently shares the bounded workspace checkpoint budget.

Workflow trash is recoverable. Archiving stops future triggers and scheduling, retains definitions/history/runs, and leaves in-flight runs to finish. Restoring never reactivates triggers. The `delete` scope is explicit; existing Operator/Builder grants are not expanded. No permanent-delete tool is exposed.

The earlier `/mcp/<workspace-id>/mcp` URL and its protected-resource discovery URL remain compatibility aliases. Existing bearer/OAuth grants stay bound to their original audience, and existing OAuth clients may refresh against that same audience through the unchanged issuer. New portal credentials and discovery use the canonical single-`mcp` path. No credential-bearing POST is redirected, and tokens are not silently accepted across both audience URLs.
