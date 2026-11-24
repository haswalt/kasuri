# Self-hosted workspace sharding

Status: **proposal, not implemented.** Until it is, the supported self-hosted topology stays one active workerd process per installation volume, as described in [scaling and availability](self-hosting.md#scaling-and-availability).

This design lets one self-hosted installation run across several workerd processes by placing each browser session's workspaces on one of N independent shards. It is the smallest step toward scale-out that reuses the current storage and execution model unchanged. It is not the full multi-worker backend described under *Future multi-worker deployment* in the self-hosting guide.

## Goals and non-goals

Goals:

- Spread workspaces across several processes, and machines, so total throughput grows with the number of shards.
- Keep each shard identical to today's single-node package: same image, same Rust/Wasm Worker, same SQLite Durable Object storage on its own volume.
- Keep existing single-node installations working without migration. An existing volume becomes shard 0.
- Keep every existing URL, webhook endpoint, MCP endpoint and session cookie valid.

Non-goals for this design:

- Scaling a single workspace. A workspace is still one Durable Object on one shard, with one storage budget (and `MAX_ACTIVE_RUNS` if set), 8 in-flight HTTP calls and single-threaded execution.
- Moving workspaces between shards automatically or while they are live.
- Failover. If a shard is down, its workspaces are unavailable until it restarts with its volume.
- Shared storage, distributed locks or work queues.

## How the code partitions today

Most state already belongs to exactly one workspace Durable Object:

| State | Where it lives |
| --- | --- |
| Workflows, revisions, runs, checkpoints, audit | The workspace's records (`crates/workflow-worker/src/storage.rs`) |
| Waits, retries, schedules, child runs | The workspace's own alarm (`WorkflowWorkspace::alarm` in `crates/workflow-worker/src/lib.rs`) |
| Data Tables and Context Store | The workspace's SQLite tables (`crates/workflow-worker/src/data.rs`) |
| Webhook registrations and token hashes | The workspace (`/hooks/<workspace>/<endpoint>`) |
| MCP credentials, OAuth clients and grants | The workspace (`/mcp/<workspace>/...`) |
| Execute Workflow and Each child runs | The same workspace, since targets are workflows in that workspace |

Every public route already names its workspace. The Worker entry point (`main` in `crates/workflow-worker/src/lib.rs`) derives the Durable Object ID from:

- the `kasuri_session=<workspace>.<token>` cookie, for the UI and `/api/*`;
- `/hooks/<workspace>/<endpoint>` and `/api/hooks/<workspace>/<workflow>/<endpoint>`, for webhooks;
- `/mcp/<workspace>/...` and the `/.well-known/oauth-*` metadata paths, for MCP and OAuth.

The one cross-workspace path is the **session's workspace directory**. The cookie names the session's home workspace, whose `workspace_memberships` table lists the other workspaces that session owns (`crates/workflow-worker/src/workspaces.rs`). The home workspace creates new workspaces with `namespace.unique_id()`, and forwards requests for a selected workspace to that Durable Object with the internal `X-Kasuri-Delegated` header, which `main` strips from every inbound request.

## Design

### Unit of placement: the session

A shard holds whole sessions: a home workspace plus every workspace created from it. Because the home workspace and its members are always on the same shard, the delegation in `workspaces.rs` stays an in-process Durable Object call, and no request ever crosses shards. This is the main simplification. It means a session's workspaces scale together, not independently.

### Shard-qualified workspace IDs

The router has to know a workspace's shard from the request alone, without a lookup service. The shard is therefore encoded in the externally visible workspace ID:

- New IDs on a sharded installation are `s<shard>-<64 hex>`, for example `s03-1f2e…`, where the hex part is the Durable Object ID exactly as today.
- A bare 64-hex ID, as used by every existing installation, means shard 0. Existing cookies, webhook URLs, MCP endpoints and OAuth audiences keep working unchanged.
- Each shard knows its own number (a new `SHARD_ID` binding) and rejects IDs for any other shard with `404`. This is required, not a convenience. Asking a shard for a Durable Object ID it does not hold would silently create an empty workspace.
- On a single-node installation (`SHARD_ID` unset), IDs stay bare, so nothing changes for current users.

Code changes on the shard side:

- **`main`:** parse and strip the prefix wherever it validates the 64-hex workspace ID today (the session cookie, `/hooks`, `/api/hooks`, `/mcp` and the OAuth metadata paths), and reject a foreign shard's prefix.
- **`workspaces.rs`:** store and return prefixed IDs in `workspace_memberships`, so the UI's workspace switcher, `X-Kasuri-Workspace` and created webhook and MCP URLs carry the shard.
- **Outward URLs:** build webhook paths, MCP resource URLs and OAuth metadata from the prefixed ID.

The session token half of the cookie, webhook tokens and other `unique_id()` entropy are unaffected.

### Router

A small stateless router sits in front of the shards. It can be a second workers-rs Worker in its own workerd process, with one `external` service binding per shard, so it stays in Rust. It:

1. Extracts the workspace ID from the request, using the same rules as `main`: the cookie, `/hooks/…`, `/api/hooks/…`, `/mcp/…`, `/.well-known/oauth-*/mcp/…`.
2. Forwards the request unchanged to that ID's shard, propagating client cancellation as `main` does today for webhook responses.
3. For a request with no session (a first visit), picks a shard from its placement policy. The shard then issues the cookie with a home ID carrying its own prefix.
4. Serves static assets from any healthy shard, or directly from the image.

Placement policy is static configuration: a list of shards, each with a weight and an `open` flag. Weighted round-robin is enough. Setting a shard to closed stops new sessions landing on it without affecting its existing workspaces. That is how a shard is drained, or kept for existing users only.

Security notes:

- **Trust:** the router never adds trusted headers. Shards keep stripping `X-Kasuri-Delegated` and generating `X-Kasuri-Session` themselves, so the trust boundary is unchanged.
- **Authentication:** Basic auth stays on the shards, and the router passes `Authorization` through. Every shard uses the same `BASIC_AUTH`, `PUBLIC_ORIGIN` and `HTTP_SECRETS`.
- **Network:** only the router should be reachable from outside. Shards authenticate requests the same way as today, so direct access is not a bypass, but a network policy keeps the topology predictable.

### Alarms, runs and recovery

Nothing changes inside a shard. Each shard runs the alarms of its own Durable Objects. A shard restart recovers its persisted runs and schedules exactly as a single-node restart does today, and other shards are unaffected. A shard that is down makes only its own workspaces unavailable, and their alarms fire late when it returns.

### Deployment

- **Kubernetes:** shards become a StatefulSet. The pod ordinal is the `SHARD_ID`, each pod gets its own volume from `volumeClaimTemplates`, and a headless Service gives each shard a stable address. Keep `Recreate`-equivalent behaviour per pod: one process per volume. The router is an ordinary Deployment, which can run several replicas because it is stateless, behind the existing Service or Ingress.
- **Compose:** one service per shard, each with its own named volume, plus a router service on the published port.
- **Adding capacity:** add a shard, mark it open in the router configuration, and reload the router. Existing workspaces stay where they are, and new sessions start landing on the new shard.
- **Backups:** back up and restore per shard volume, as today. An installation backup is the set of all shard volumes plus the router configuration.
- **Existing installations:** keep the current volume as shard 0, put the router in front, and add shards. No data moves.

## Limits that remain

- **Per-workspace limits:** a single busy workspace is still capped by one Durable Object (one storage budget, 8 in-flight HTTP calls, one thread). Sharding only raises the total across workspaces.
- **Co-located sessions:** all workspaces of one session share a shard.
- **No rebalancing:** a hot shard can be closed to new sessions but not emptied. Moving a workspace changes its ID, and therefore its URLs and credentials, so it is an export and import, not a move.
- **No failover:** losing a shard means downtime for its workspaces until it is restarted with its volume.

## Open questions to verify before implementing

- **Durable Object IDs across processes:** confirm that `unique_id()` and `id_from_string()` with the same `uniqueKey` behave identically in separate workerd processes, so the 64-hex part keeps its meaning. Also confirm that nothing in the ID ties it to the creating process.
- **Router forwarding:** confirm workerd `external` service bindings preserve streaming request bodies, cancellation and `Set-Cookie` handling well enough for webhook response windows and OAuth redirects.
- **Health and readiness:** decide what the router reports when one shard is down. Probably healthy, with per-shard status, so one shard cannot take the whole installation out of rotation.
- **Identity:** align with [third-party platform authentication](platform-auth.md). When sessions move from cookies to provider identities, the unit of placement should become the principal or organization rather than the browser session.

## Alternatives considered

- **`hash(id) mod N` routing:** needs no ID change, but changing N moves every workspace to a different shard, so shards could never be added. Rejected.
- **A router-side directory (workspace → shard table):** allows arbitrary placement and moves, but makes the router stateful and a single point of failure that needs its own replication and backups. It could be added later for moves while keeping prefixed IDs as the default.
- **Sharing one SQLite volume between processes:** unsafe. There is no cross-process ownership or locking. Rejected, as in the self-hosting guide.
- **workerd Durable Object clustering:** not something the pinned workerd configuration provides or that Kasuri has verified. Revisit if upstream support appears.
- **A full distributed backend** (shared storage, leases with fencing, durable queues, alarm ownership): the only option that scales single workspaces and survives node loss. It is much larger and remains the long-term path described in the self-hosting guide.

## Implementation phases and acceptance

1. **Shard-aware IDs on the shard.** Add `SHARD_ID`, prefixed ID parsing and generation, and foreign-prefix rejection. With `SHARD_ID` unset, all existing runtime checks must pass unchanged.
2. **Router.** Add the router Worker, its configuration format and the placement policy.
3. **Packaging.** Add the StatefulSet and router manifests, a Compose variant, and updated self-hosting docs.
4. **Multi-process runtime checks** (a new `scripts/` suite running two or more real shard processes behind the router), covering:
   - first visits spread across open shards, and closed shards receive none;
   - workspace creation and switching stay on the home shard;
   - webhook deliveries, MCP calls and OAuth flows reach the correct shard by URL alone;
   - a foreign or forged prefix is rejected and never creates an empty workspace;
   - bare legacy IDs reach shard 0;
   - restarting one shard recovers its waits and schedules while other shards keep serving;
   - per-shard backup and restore.
