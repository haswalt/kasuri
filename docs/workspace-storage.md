# Scalable workspace storage and execution

Status: **stage 1 implemented.** Every workspace is stored in per-entity tables (workspaces from earlier releases migrate on first load), finished runs are archived out of memory, and run history follows configurable retention policies (see [run history](deployment.md#run-history)). What a workspace holds in memory is bounded by a 6 MB budget that protects the Durable Object's memory, not its storage. Stage 2 is not started.

Priorities, in order: **scalability** (concurrent runs and history grow with demand, not with one object's limits), **performance** (an operation's latency and CPU don't grow with workspace size), then **cost** (no work or writes beyond what an operation changes).

The design has two stages:

1. **Per-entity storage.** Replace the whole-workspace document with per-entity rows that are loaded on demand and written only when they change. This removes the 1.5 MB workspace budget and makes each operation's cost proportional to what it touches.
2. **Queue-driven execution.** Run steps execute in stateless queue consumers, which scale out like any Worker, instead of inside the workspace's single Durable Object. Run state lives in a store with compare-and-swap, so duplicate deliveries are harmless, and both the queue and the store sit behind interfaces so another bus (SQS, RabbitMQ, NATS) can replace Cloudflare Queues. The workspace object becomes a control plane: definitions, publication, credentials, the run index and admission.

The primary target is Cloudflare; self-hosting is a compatibility constraint. Stage 1 is a prerequisite for stage 2 and is useful on its own. Stage 2 is what makes a single busy workspace scale.

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

- **Scale:** a workspace sustains thousands of concurrently active runs (stage 2), limited by configured policy (`MAX_ACTIVE_RUNS`) rather than by one object's thread, storage or memory.
- **Latency:** webhook acceptance, run steps and API reads take time independent of how many runs a workspace has, active or retained.
- **Writes:** an operation writes only the rows it changed, in one transaction.
- **Reads:** an operation loads only the entities it needs, and listing runs reads summary columns, not checkpoints.
- **Budgets:** per-item limits (run checkpoint, value, request) replace the per-workspace byte budget. Total size is bounded by retention policy and the Durable Object's 10 GB storage quota.
- **Safety:** atomicity is unchanged. An operation's writes commit together or not at all.
- **Compatibility:** the engine, checkpoint format, HTTP API, MCP tools and export format are unchanged. Existing workspaces migrate in place.
- **One codebase:** the same Rust service runs on Cloudflare and self-hosted, with native tests that need no SQLite.

Non-goals:

- **Spreading one run:** a single run stays on one object. Parallel branches inside a run still share that run's thread, bounded by the run's own concurrency settings.
- **Engine and checkpoint changes:** no change to engine semantics or to the run checkpoint format.
- **Sharding:** no change to [self-hosted sharding](self-host-sharding.md). Each shard simply gets the new layout.

## Stage 1: per-entity storage

### Tables

All tables live in the workspace Durable Object's SQLite database, next to the existing Data Tables tables (`crates/workflow-worker/src/data.rs`), which already work this way.

As implemented in phase 3 (`crates/workflow-service/src/store_sql.rs`):

| Table | Key | Columns | Notes |
| --- | --- | --- | --- |
| `ws_meta` | `key` | `value` | `format` (the layout version), `commit` (a token that changes with every commit, so the worker can tell whether its decoded copy is current) and `workspace` (everything that isn't a keyed collection, including `management`, which is already bounded: 64 credentials, 128 clients, 64 requests, 64 receipts). |
| `ws_workflows` | `id` | `ord`, `body` | One row per workflow document. |
| `ws_revisions` | `id` (`workflow/revision`) | `ord`, `body` | Published graphs. |
| `ws_runs` | `id` | `ord`, `status`, `created_at`, `wake_at`, `response_open`, `response_deadline`, `parent_id`, `root_id`, `summary`, `body` | One row per run. `body` is the `RunRecord`. The other columns are derived from it on every write (`RunColumns`). |
| `ws_run_awaits` | `(child_id, run_id)` | | Child runs each run awaits and hasn't collected, for retention protection. |
| `ws_audit` | `id` | `ord`, `body` | Append-only. |
| `ws_webhooks` | `id` (endpoint) | `ord`, `body` | Webhook registrations. The `webhook_auth` descriptor table from haswalt/kasuri#14 is still written alongside, whenever a workflow, revision or webhook changes. |

`ord` keeps each collection's document order with sparse keys, so prepending a run or appending an audit entry never rewrites other rows.

Indexes: `ws_runs(ord, id)`, `ws_runs(status)`, `ws_runs(wake_at)`, `ws_runs(response_open, response_deadline)`, `ws_runs(parent_id)`, `ws_runs(root_id)` and `ws_run_awaits(run_id)`. A native test checks each run query's SQLite plan: none visits every run, and run lists come out of the order index without sorting.

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

- **Per run:** a run's `body` must fit in one row. Durable Object SQLite allows rows of up to 8 MB, so the engine's existing 2 MiB checkpoint budget fits without change. Value budgets (64 KiB per input and output) and request budgets are unchanged.
- **Per workspace:** no storage budget. Total size is bounded by retention and by the Durable Object's storage quota of 10 GB.
- **Retention becomes policy.** *Implemented:* finished runs are kept up to a configurable count per workflow (`RUN_HISTORY`, default 100) and age (`RUN_HISTORY_DAYS`), with a separate age for failed runs (`FAILED_RUN_HISTORY_DAYS`). Pruning is indexed `DELETE`s in the same transaction as the commit: by count for workflows whose runs were just archived (`ws_runs_workflow`), by age on every commit (`ws_runs_age`). Runs still in memory, which includes everything an active run depends on, are never deleted. Audit entries stay in memory, bounded by count (1,000) and by the memory budget.
- **Memory:** the service still operates on a `Workspace` held in memory, and a Durable Object has about 128 MB. So before any byte budget can go, history has to leave memory. A finished run that no run in memory depends on (no active parent, invocation root or awaiting caller, and no open webhook response) is **archived** when its operation commits: its row stays in `ws_runs` with `archived = 1`, and loads skip it. Run lists, run details, MCP run tools and closed webhook replies read archived runs through `RunArchive`, which the worker attaches to every loaded workspace. Memory then holds workflows, active runs and audit entries, whatever the history size.
- **Memory budget:** `MAX_STATE_BYTES` now bounds the in-memory state rather than storage, at 6 MB, with admission at 75%. It stays because memory, not storage, is the real limit while the service works on a whole in-memory workspace. Measured on workerd with active runs holding 50 KB inputs, a request's memory grew about ten times the encoded state (decoded copies, the cached committed copy and per-entity encodings), and each run's stored state was about three times its input. 6 MB keeps a full workspace well inside 128 MB. Reducing that multiplier, storing inputs once, and loading active runs on demand would each raise it; stage 2 removes it.
- **Concurrency:** `MAX_ACTIVE_RUNS` (from haswalt/kasuri#27) remains the count control.

### Migration

On the first load after upgrading, when `ws_meta` has no format version, the workspace migrates in **one transaction** (`store::sql::legacy::migrate`):

1. Read the current state: the latest record generation, or the legacy single row. A corrupt generation is an error, never an empty workspace.
2. Insert every entity into the tables, derive the run columns, rewrite `webhook_auth`, set the `ws_meta` format version, and drop `workspace_records` and the legacy `workspace` table.
3. If anything fails, the transaction rolls back and the old layout is left exactly as it was. The next load tries again.

Native tests migrate both legacy layouts on SQLite with the frozen SQL those releases wrote, and inject a failure at every statement of a migration to show that each one rolls back to the intact old layout.

Notes:

- **Downgrade:** releases before phase 3 can't read the tables. Phase 3 can migrate tables back to generations (`WORKSPACE_STORAGE=generations`); from phase 4 on, migration is one-way and `WORKSPACE_STORAGE` is no longer read.
- **Size:** the migration's own transaction rewrites at most the current 1.5 MB workspace, so it stays within today's limits.
- **Backups:** backups remain whole-volume or whole-object, so the procedures in the self-hosting guide don't change.

## Stage 2: queue-driven execution

### Why stage 1 isn't enough

Stateless Workers scale out automatically, but Kasuri doesn't run workflows in them. The entry Worker only authenticates and routes requests to the workspace's Durable Object, and runs execute inside that object:

- starting runs and advancing them on its alarms (`WorkflowWorkspace::alarm`);
- dispatching HTTP calls;
- Data Tables;
- webhook responses.

A Durable Object is a single instance: single-threaded, in one location, with a soft limit of about 1,000 requests per second. Stage 1 makes each operation cheap, but one busy workspace is still capped by that one instance. Stage 2 moves run execution into stateless Workers, fed by a message queue, so it scales the way Workers do.

### Requirements

- **Cloudflare first.** The design targets Workers, Durable Objects and Queues. Self-hosting is a compatibility constraint, not a design driver.
- **A replaceable bus.** Execution must not depend on Cloudflare Queues specifically. The same design must run on SQS, RabbitMQ, NATS JetStream or similar, behind a small adapter.
- **Correct under at-least-once delivery.** Queues (like most buses) may deliver a message more than once or out of order. Effects must still happen exactly as often as they do today.

### Shape

| Component | Role |
| --- | --- |
| **Workspace Durable Object** (control plane) | Workflows, revisions, publication, schedules, webhook registrations, MCP and OAuth state, audit, the run index, and admission (`MAX_ACTIVE_RUNS`). It no longer advances runs. |
| **Run store** | The durable state of each run: its checkpoint (stage 1's `ws_runs` row format), version and pending timers. |
| **Work queue** | Carries small "this run has work" messages. On Cloudflare this is a Queue, consumed by the same Worker. |
| **Step workers** | Stateless queue consumers. Each message loads one run, advances it with the existing engine, executes the dispatched task (HTTP, data, child start), and saves the run. |
| **Timer table** | Due times for waits and retries longer than the bus can delay, swept by a scheduler. |

Two interfaces make the bus and the store replaceable:

```rust
/// A message bus with at-least-once delivery and optional delay.
pub trait WorkBus {
    fn send(&self, message: WorkMessage, delay: Option<Duration>) -> Result<(), BusError>;
}
/// Durable run state with compare-and-swap on a version.
pub trait RunStore {
    fn load(&self, run: &RunId) -> Result<Option<(RunRecord, Version)>, StoreError>;
    fn save(&self, run: &RunId, record: &RunRecord, expected: Version) -> Result<Version, StoreError>;
}
```

- **On Cloudflare:** `WorkBus` sends to a Queue. `RunStore` is one Durable Object per run, holding that run's row, called over RPC. Each run object is a separate instance, so runs don't contend with each other or with the workspace.
- **Elsewhere:** `WorkBus` maps to SQS, RabbitMQ or NATS. `RunStore` maps to Postgres (or another SQL store) with a version column, used for compare-and-swap.

### Messages

A message identifies work and never carries data:

```json
{ "workspace": "…", "run": "…", "version": 17, "reason": "advance" }
```

- **Small and portable:** well under Cloudflare's 128 KB message limit and every common bus's. Inputs, outputs and checkpoints stay in the run store.
- **Always re-read:** a consumer loads the current run and works from that, not from the message, so stale and duplicate messages are harmless.

### Processing a message

1. **Load** the run and its version from the run store. If the run has already moved past the message's version, or is terminal, acknowledge the message and stop.
2. **Advance** it with the existing engine (`Run::next`), which returns either a task to dispatch or a wake time.
3. **Persist the dispatch marker** with compare-and-swap before doing anything external, exactly as the workspace does today. If the swap fails, another consumer owns this step: acknowledge and stop.
4. **Execute the task.** HTTP calls run in the consumer Worker. The task's existing idempotency key (`Task::idempotency_key`) is sent with every external effect.
5. **Record the outcome** with compare-and-swap, then send the next message: immediately, or with a delay for waits and retries.
6. **Report status changes** (started, paused, succeeded, failed, cancelled) to the workspace index, idempotently, keyed by run and version.

Each run's steps go through one compare-and-swap at a time. That gives the same ordering guarantees as today's single-object execution, without a lock service. Different runs progress fully in parallel across consumer invocations.

### Timers

Waits and retries are delays on the next message.

- **Short delays:** delays within the bus's limit (24 hours on Cloudflare Queues, 15 minutes on SQS) are sent as delayed messages.
- **Long delays:** Wait / Delay has no upper bound (`delayMs` only has to be 0 or more), so longer delays are written to the timer table. A scheduler sends their messages when they come due. On Cloudflare that's a Cron Trigger or a Durable Object alarm, and elsewhere any cron. The timer row is written in the same compare-and-swap as the run, so a timer is never lost.

### Children, Each and webhook responses

- **Execute Workflow / Each children:** the parent's consumer asks the workspace to admit the child, writes the child run, and sends its first message. When the child finishes, its final step sends an "advance" message to the parent, keyed by the parent task's idempotency key. Each fan-out becomes many independent runs processed in parallel, bounded by the node's concurrency setting.
- **Webhook response windows:** a delivery that waits for a Respond to Webhook node is held by a small per-request Durable Object (the waiter). The step that decides the response calls the waiter. Waiters only last for the response window (5 seconds), so they don't become a bottleneck.
- **Data Tables and Context Store:** data operations are already explicit, transactional commands with receipts (`crates/workflow-worker/src/data.rs`). Consumers call them over RPC on per-dataset Durable Objects on Cloudflare, or on SQL transactions elsewhere.

### Throughput on Cloudflare

| Limit | Value | Effect |
| --- | --- | --- |
| Consumer invocations per queue | 250 concurrent, each taking batches of up to 100 messages | Thousands of run steps in flight at once |
| Messages per queue | 5,000 per second | Use several queues (for example, sharded by workspace or run) for more |
| Consumer CPU | Up to 5 minutes per invocation | Per-step CPU is generous; steps normally take milliseconds |
| Durable Objects | Unlimited objects, about 1,000 requests per second each | Per-run objects never approach the per-object limit |

Admission (`MAX_ACTIVE_RUNS`) and per-node concurrency remain the controls for how much work a workspace generates. Queue consumers can also be capped (`max_concurrency`) to protect downstream APIs.

### Cost

- **Requests:** each step costs one queue message and one run-object request. Today it costs one alarm and a whole-workspace rewrite. Writes fall sharply.
- **Idle runs cost nothing:** waits are delayed messages or timer rows, not objects held in memory. A run object only runs during a step.
- **Duration:** HTTP calls run in consumer Workers, which bill CPU rather than wall time. Today a workspace Durable Object waiting on HTTP accrues duration for up to 15 minutes per pending call.

### Self-hosting

Self-hosting implements `WorkBus` and `RunStore` with whatever it has: a small in-process queue plus SQLite for a single node, or a real bus plus Postgres to scale out. That replaces the per-workspace sharding design as the way to scale self-hosted, because state no longer lives in one process.

## Risks

- **Change tracking:** a mutation that forgets to mark its entity as changed would be lost silently. Mitigations:
  - make every write go through `Workspace` methods that record the change;
  - have the in-memory store assert in tests that each operation's result equals a full reload after commit;
  - run the existing suites against both stores.
- **Invariants spanning runs:** parent and child relations, Each iteration state and webhook response windows currently rely on scanning one in-memory list. Each one needs an explicit query and a test.
- **Ordering:** audit order, run list order and pagination are API-visible. They need explicit `ORDER BY` clauses and keys.
- **At-least-once delivery (stage 2):** every handler must be idempotent and version-checked, and external effects must carry idempotency keys. A test harness should deliver every message twice, out of order and after crashes.
- **Eventual consistency (stage 2):** run lists and `MAX_ACTIVE_RUNS` admission read the workspace's index, which lags runs by one status report. Admission must count runs it has admitted but not yet heard from, so the cap is never exceeded. Lists need a documented staleness bound.
- **Per-step latency (stage 2):** each step adds a queue hop and a run-store round trip, so a single run's steps may be slightly slower while total throughput rises a lot. Short chains of fast nodes could run several steps per message to compensate. Measure both.
- **Scope:** `workflow-service` accesses `self.runs` in about 67 places and `self.workflows` in about 29. They move behind the store in phases rather than in one change.

## Phases and acceptance

1. **Store trait with an in-memory implementation** that reproduces today's behaviour, with no storage format change. All existing tests pass through it.
2. **Run queries:** move run lookups, listing, due-run selection, `next_wake`, and invocation and Each queries onto the trait. Add the summary columns.
3. **SQLite store and migration** in the worker. The old layout is still readable for rollback if that option is chosen. *Implemented:* `SqlStore` over a minimal SQL interface (Durable Object SQLite in the worker, SQLite in native tests), opt-in with `WORKSPACE_STORAGE=tables`, migrating in both directions.
4. **Incremental commits** become the default. Record generations, the decode cache and whole-document `checkpoint()` are removed. *Implemented:* tables are the only layout and older layouts migrate one way. Saves no longer encode the whole workspace: retention works from the per-entity encodings the commit already produces (`Workspace::retain`), and a test checks it removes exactly what whole-document retention would. The generation decode cache is gone; the worker keeps the last committed workspace, keyed by commit token, until entities are loaded on demand. A save still encodes every entity to find what changed, and admission still measures the workspace; on-demand loading and phase 5 remove both.
5. **Retention policies** replace `MAX_STATE_BYTES`, and the admission guard is removed or replaced with the indexed active-bytes check. *Implemented:* finished runs are archived out of memory (format 2 adds `ws_runs.archived` and `ws_runs.workflow_id`), history follows `RUN_HISTORY`, `RUN_HISTORY_DAYS` and `FAILED_RUN_HISTORY_DAYS` (format 3 adds the age index; older tables upgrade in place), and `MAX_STATE_BYTES` and its admission guard now protect memory at 6 MB instead of storage at 1.5 MB, because measurement showed memory is the binding limit (see budgets above).
6. **`WorkBus` and `RunStore`** (stage 2): define both, with Cloudflare Queues and per-run Durable Object implementations plus in-memory ones for tests. Move top-level runs onto queue-driven execution behind a flag.
7. **Timers, children and Each:** the timer table and scheduler, child admission and completion messages, and Each fan-out as independent runs.
8. **Data and webhook responses:** data operations behind per-dataset objects, and webhook response waiters.
9. **A second bus:** one non-Cloudflare `WorkBus` and `RunStore` implementation (for example SQS or NATS with Postgres), to prove the interfaces are portable.

Acceptance:

- **Constant cost:** an alarm tick or webhook delivery writes a constant number of rows regardless of workspace size, and its duration doesn't grow with the number of retained runs. Measure at 100, 1,000 and 10,000 retained runs.
- **Concurrency:** 200 concurrently active runs with 50 KB outputs each succeed, which is impossible today, and remain cancellable.
- **Migration:** workspaces from every existing format (legacy row, record generations, with and without `webhook_auth`) migrate. A failure injected mid-migration leaves the old layout intact.
- **Scale (stage 2):** 1,000 concurrently active runs in one workspace with median step latency within 2× of a single isolated run, and webhook acceptance latency independent of active-run count.
- **Recovery (stage 2):** duplicating, reordering and dropping-then-redelivering every message, killing consumers mid-step and restarting the workspace object never loses a run or repeats a completed effect.
- **Regression:** the full `pnpm test:runtime` suite passes unchanged, including storage atomicity, restart persistence and webhook response windows.
- **Change tracking:** every operation in the native suites matches a fresh reload after commit.

## Alternatives considered

- **Raise `MAX_STATE_BYTES`:** quick, but it keeps whole-document cost per operation and makes every request slower as the budget fills. It can't go far past the per-row limit for a single run anyway. Not a fix.
- **Keep the document but write only the records that changed:** this removes the write amplification but not the load cost or the budget. Useful as an interim step (phase 4 without phases 1–3) if the full change is too large at once.
- **One Durable Object per run, driven by its own alarms (no queue):** scales well on Cloudflare and needs no bus. But execution and state would both be tied to Durable Objects, with no portable equivalent, and HTTP calls would wait inside Durable Objects (billed by wall time) instead of in consumer Workers. Rejected in favour of the portable queue design. Per-run objects are still used, but only as the run store on Cloudflare.
- **Cloudflare Workflows:** durable steps, sleeps and retries, up to 50,000 concurrent instances per account. But the documented entrypoints are JavaScript, TypeScript and Python only, so Kasuri's Rust engine would need a JavaScript wrapper. Its step model would also duplicate the engine's own retry, wait and checkpoint semantics, and there's no equivalent outside Cloudflare. Rejected for portability and to keep a single execution model.
- **Run shards** (N execution objects per workspace): fewer objects, but each shard is still one thread shared by its runs and needs rebalancing as load changes. The queue design scales without tuning.
