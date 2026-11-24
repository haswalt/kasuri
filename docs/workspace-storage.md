# Workspace storage: per-entity rows instead of one document

Status: **proposal, not implemented.** Until it is, the limits in [deployment](deployment.md) apply: a 1.5 MB budget per workspace and retention of up to 100 runs.

This design replaces the whole-workspace document with per-entity rows that are loaded on demand and written only when they change. It removes the 1.5 MB workspace budget as a limit on concurrency and history, and makes the cost of an operation proportional to what it touches rather than to the size of the workspace.

## The problem

Each workspace is one Durable Object, and the service treats its whole state as a single `Workspace` value (`crates/workflow-service/src/lib.rs`). That value holds:

| Field | Contents |
| --- | --- |
| `workflows`, `revisions` | Every workflow document and every published revision |
| `runs` | Up to 100 runs, each with its full engine checkpoint (node states and outputs) |
| `audit`, `audit_sequence` | Up to 1000 audit entries |
| `webhooks`, `webhook_events` | Endpoint registrations (`webhook_events` is no longer written) |
| `management` | MCP credentials, OAuth clients, grants and receipts |

Every request, webhook delivery, MCP call and alarm tick follows the same cycle:

1. **Load:** read every record and decode the whole document (`WorkflowWorkspace::load` in `crates/workflow-worker/src/storage.rs`).
2. **Change:** usually one run or one workflow.
3. **Save:** serialise the whole document, check it against `MAX_STATE_BYTES` (1.5 MB), then insert a complete new generation of every record and delete the previous one.

The 1.5 MB budget dates from when the workspace was stored as one SQLite row (`workspace (id=1, body TEXT)`), which needed to stay under the per-row size limit. Storage moved to one row per record in `8093b34`, but the budget and the whole-document cycle stayed.

Consequences:

- **Concurrency:** active runs cannot be pruned, so all in-flight runs' outputs must fit in 1.5 MB together with every workflow, revision and audit entry. A single run's checkpoint may legally reach 2 MiB (`MAX_CHECKPOINT_BYTES` in the engine), larger than the whole workspace budget.
- **History:** retention deletes the oldest runs and audit entries to stay under the budget, not because of any policy.
- **Cost per operation:** advancing one node in one run re-encodes and rewrites everything. Durable Object storage bills writes per row, so cost grows with workspace size.
- **CPU:** encoding and decoding spend the per-request CPU allowance. The decode cache (haswalt/kasuri#15) and cached run identity (haswalt/kasuri#12) reduce this, but saves still rewrite everything.

## Goals and non-goals

Goals:

- **Writes:** an operation writes only the rows it changed, in one transaction.
- **Reads:** an operation loads only the entities it needs, and listing runs reads summary columns, not checkpoints.
- **Budgets:** per-item limits (run checkpoint, value, request) replace the per-workspace byte budget. Total size is bounded by retention policy and the Durable Object's storage quota.
- **Safety:** atomicity is unchanged. An operation's writes commit together or not at all.
- **Compatibility:** the engine, checkpoint format, HTTP API, MCP tools and export format are unchanged. Existing workspaces migrate in place.
- **One codebase:** the same Rust service runs on Cloudflare and self-hosted, with native tests that need no SQLite.

Non-goals:

- **Spreading one workspace:** a workspace is still one Durable Object, so one thread on one machine. Splitting it, for example one Durable Object per run, is a later step that this design makes possible.
- **Engine and checkpoint changes:** no change to engine semantics or to the run checkpoint format.
- **Sharding:** no change to [self-hosted sharding](self-host-sharding.md). Each shard simply gets the new layout.

## Design

### Tables

All tables live in the workspace Durable Object's SQLite database, next to the existing Data Tables tables (`crates/workflow-worker/src/data.rs`), which already work this way.

| Table | Key | Columns | Notes |
| --- | --- | --- | --- |
| `ws_meta` | `key` | `value` | Format version, `audit_sequence`, and the `management` value. Management stays one row: it is already bounded (64 credentials, 128 clients, 64 requests, 64 receipts). |
| `ws_workflows` | `id` | `version`, `status`, `archived_at`, `updated_at`, `body` | One row per workflow document. |
| `ws_revisions` | `(workflow_id, revision)` | `created_at`, `body` | Published graphs. |
| `ws_runs` | `id` | `workflow_id`, `status`, `mode`, `created_at`, `wake_at`, `root_id`, `parent_id`, `webhook_open`, `body_bytes`, `summary`, `body` | One row per run. `body` is today's `RunRecord`. The other columns are derived from it on every write and indexed for queries. |
| `ws_audit` | `seq` | `at`, `body` | Append-only. |
| `ws_webhooks` | `endpoint_id` | `workflow_id`, `token_hash` | Absorbs the `webhook_auth` descriptor table from haswalt/kasuri#14. |

Indexes: `ws_runs(status)`, `ws_runs(wake_at)`, `ws_runs(created_at)`, `ws_runs(root_id)` and `ws_runs(parent_id)`.

`webhook_events` is dropped. Nothing writes it, and it has been empty since webhook deduplication moved into workflows.

The `summary` column holds what run lists show today (`RunRecord::view(true)`), so listing runs never decodes a checkpoint. The derived columns and `summary` are written by the same code path that writes `body`, extending `RunRecord::store` from haswalt/kasuri#12, so they cannot drift from it.

### A storage interface under the service

The service stays pure Rust and host-independent. It gains a small storage trait in `workflow-service`, and `Workspace` becomes a **unit of work** over it rather than the whole state.

Sketch, not final:

```rust
pub trait Store {
    fn meta(&self, key: &str) -> Result<Option<Value>, StoreError>;
    fn workflow(&self, id: &str) -> Result<Option<Document>, StoreError>;
    fn workflows(&self, filter: WorkflowFilter) -> Result<Vec<WorkflowSummary>, StoreError>;
    fn revision(&self, workflow: &str, revision: u32) -> Result<Option<Revision>, StoreError>;
    fn run(&self, id: &str) -> Result<Option<RunRecord>, StoreError>;
    fn runs(&self, filter: RunFilter, page: Page) -> Result<Vec<RunSummary>, StoreError>;
    fn due_runs(&self, now: u64, limit: usize) -> Result<Vec<String>, StoreError>;
    fn next_wake(&self) -> Result<Option<u64>, StoreError>;
    fn webhook(&self, endpoint: &str) -> Result<Option<WebhookRegistration>, StoreError>;
}
```

- **Reads:** `Workspace` reads entities through the store on first use and keeps them for the rest of the operation.
- **Writes:** it records which rows it inserted, changed or deleted.
- **Commit:** `commit()` returns that change set, and the host applies it in one transaction (`transactionSync`, as today).

Two implementations:

- **SQLite** (`workflow-worker`): the tables above. It replaces record generations, the decode cache and `checkpoint()`'s whole-document encode.
- **In memory** (`workflow-service` tests): `BTreeMap`s, so the existing native test suites keep running without SQLite.

Atomicity needs no generations. A Durable Object processes its storage operations one transaction at a time, so a synchronous transaction is never seen half-applied.

### Hot paths after the change

| Operation | Reads | Writes |
| --- | --- | --- |
| Webhook delivery | `ws_webhooks` row, workflow and live revision | One new `ws_runs` row and the trigger run's progress |
| Alarm tick | `due_runs(now)`, plus each due run's row | Only runs that advanced, and `next_wake` is a `MIN(wake_at)` query |
| HTTP completion | One run row | That run row |
| Save a workflow | Workflow row (plus webhook rows for auth carry-over) | Workflow row, audit row |
| Publish | Workflow row | Workflow row, revision row, audit row |
| List runs | `ws_runs.summary` with paging | Nothing |
| Run details | One run row | Nothing |
| Execute Workflow / Each child | Parent run row, root run row (for `child_count`), revision | Child run row, parent run row |

Invocation rules that scan every run today, such as `invocation_protected` and child budgets, become indexed queries on `root_id`, `parent_id` and `status`.

### Budgets and retention

- **Per run:** a run's `body` must fit in one SQLite row. As I understand Cloudflare's Durable Object SQLite limits, that's about 2 MB per row (to confirm). The engine currently allows a 2 MiB checkpoint, slightly more, so the per-run budget must be set just under the row limit, for example 1.9 MB including `RunRecord` metadata. Value budgets (64 KiB per input and output) and request budgets are unchanged.
- **Per workspace:** no byte budget. `MAX_STATE_BYTES` and the storage admission guard from haswalt/kasuri#27 are removed. Total size is bounded by retention and by the Durable Object's storage quota, which is measured in gigabytes rather than megabytes.
- **Retention becomes policy.** Terminal runs are kept up to a configurable count and age, per workflow, plus failed runs for longer. Audit entries are kept up to a count and age. Retention runs as indexed `DELETE`s after commits or on alarms, never by encoding the workspace. Runs that active parents or open webhook responses depend on stay protected, as today.
- **Concurrency:** `MAX_ACTIVE_RUNS` (from haswalt/kasuri#27) remains the count control. If a storage-based safeguard is still wanted, it becomes `SUM(body_bytes)` over active runs against a fraction of the quota. That is one indexed query.

### Migration

On the first load after upgrading, when `ws_meta` has no format version:

1. Read the current state as today: the latest record generation, or the legacy single row.
2. In **one transaction**, insert every entity into the new tables, derive the run columns, set the `ws_meta` format version, and delete `workspace_records`, `webhook_auth` and the legacy `workspace` table.
3. If anything fails, the transaction rolls back and the workspace keeps running on the old layout until the next attempt.

Notes:

- **Downgrade:** going back to a release without the new layout would not be able to read migrated workspaces. Either ship the migration in a release that can still read and write the old layout (kept for one release), or document downgrade as unsupported after migration.
- **Size:** the migration's own transaction rewrites at most the current 1.5 MB workspace, so it stays within today's limits.
- **Backups:** backups remain whole-volume or whole-object, so the procedures in the self-hosting guide don't change.

## Risks

- **Change tracking:** a mutation that forgets to mark its entity as changed would be lost silently. Mitigations:
  - make every write go through `Workspace` methods that record the change;
  - have the in-memory store assert in tests that each operation's result equals a full reload after commit;
  - run the existing suites against both stores.
- **Invariants spanning runs:** parent and child relations, Each iteration state and webhook response windows currently rely on scanning one in-memory list. Each one needs an explicit query and a test.
- **Ordering:** audit order, run list order and pagination are API-visible. They need explicit `ORDER BY` clauses and keys.
- **Scope:** `workflow-service` accesses `self.runs` in about 67 places and `self.workflows` in about 29. They move behind the store in phases rather than in one change.

## Phases and acceptance

1. **Store trait with an in-memory implementation** that reproduces today's behaviour, with no storage format change. All existing tests pass through it.
2. **Run queries:** move run lookups, listing, due-run selection, `next_wake`, and invocation and Each queries onto the trait. Add the summary columns.
3. **SQLite store and migration** in the worker. The old layout is still readable for rollback if that option is chosen.
4. **Incremental commits** become the default. Record generations, the decode cache and whole-document `checkpoint()` are removed.
5. **Retention policies** replace `MAX_STATE_BYTES`, and the admission guard is removed or replaced with the indexed active-bytes check.

Acceptance:

- **Constant cost:** an alarm tick or webhook delivery writes a constant number of rows regardless of workspace size, and its duration doesn't grow with the number of retained runs. Measure at 100, 1,000 and 10,000 retained runs.
- **Concurrency:** 200 concurrently active runs with 50 KB outputs each succeed, which is impossible today, and remain cancellable.
- **Migration:** workspaces from every existing format (legacy row, record generations, with and without `webhook_auth`) migrate. A failure injected mid-migration leaves the old layout intact.
- **Regression:** the full `pnpm test:runtime` suite passes unchanged, including storage atomicity, restart persistence and webhook response windows.
- **Change tracking:** every operation in the native suites matches a fresh reload after commit.

## Alternatives considered

- **Raise `MAX_STATE_BYTES`:** quick, but it keeps whole-document cost per operation and makes every request slower as the budget fills. It can't go far past the per-row limit for a single run anyway. Not a fix.
- **Keep the document but write only the records that changed:** this removes the write amplification but not the load cost or the budget. Useful as an interim step (phase 4 without phases 1–3) if the full change is too large at once.
- **One Durable Object per run:** spreads execution across machines, but adds cross-object coordination for invocation, Each and webhook response windows. It is a later step, and much easier once runs are separate rows with explicit relations.
