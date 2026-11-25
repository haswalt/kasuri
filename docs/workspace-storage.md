# Scalable workspace storage and execution

Workspace data and run trees are stored in SQLite-backed Cloudflare Durable Objects. Finished runs are archived out of memory according to the [run history policy](deployment.md#run-history). Workspace commits write changed rows but still scan resident entities. Run objects use SQLite and advance through Durable Object alarms by default or through Cloudflare Queues when configured.

## Original storage bottleneck

Before the per-entity storage migration:

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
- **One codebase:** the Rust service runs on Cloudflare and is exercised by native tests.

Non-goals:

- **Spreading one run:** a single run stays on one object. Parallel branches inside a run still share that run's thread, bounded by the run's own concurrency settings.
- **Engine and checkpoint changes:** no change to engine semantics or to the run checkpoint format.

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
- **Memory budget:** `MAX_STATE_BYTES` now bounds the in-memory state rather than storage, at 6 MB, with admission at 75%. It stays because memory, not storage, is the real limit while the service works on a whole in-memory workspace. Measured on workerd with active runs holding 50 KB inputs, a request's memory grew about ten times the encoded state (decoded copies, the cached committed copy and per-entity encodings). The committed copy now keeps a SHA-256 digest of each row rather than its text, reads copy only the workspace, and writes take the cached state instead of copying it, which brought that to about eight times. 6 MB keeps a full workspace well inside 128 MB. Reducing that multiplier and loading active runs on demand would each raise it; stage 2 removes it.
- **Compact run state:** checkpoints store each repeated value of 1 KiB or more once (`workflow_engine::compact`): data that passes through a trigger, a Wait node or a router used to be stored again for every node. A run holding a 50 KB input stored about 150 KB before and 50 KB after, so the same budget admits about 85 such runs instead of 28. The 2 MiB per-run budget still applies to the full, expanded state, plain checkpoints from earlier releases still load, and expansion rejects reference cycles and anything that would expand past the run budget.
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
- **Backups:** preserve both the workspace and run Durable Object namespaces when upgrading.

## Run objects

### Alarm scheduling

Before Queues carry work, runs move out of the workspace onto **one Durable Object per run tree**, whose own alarm is the bus. This works the same on Cloudflare and on workerd, needs no extra resources, and takes runs out of the workspace's memory (on-demand loading). It is selected with `RUN_EXECUTION=objects` or an unset/empty value. Objects became the default after Cloudflare functional verification on 25 November 2026 (see deployment documentation); `workspace` remains an explicit fallback and `queues` an optional bus.

- **Eligible runs:** every top-level run except simulations that use data operations, directly or in a workflow they may invoke (`execution::eligible`); those keep their data snapshot in the workspace. A run that invokes other workflows moves with its children as one run tree (see "Run trees"). Data operations and webhook responses are covered in "Data and webhook responses".
- **Starting:** the workspace starts the run as always. Before each commit the host calls `Workspace::offload_waiting`, which marks runs that are waiting (not on a data task, with no HTTP request in flight from the workspace) as leaving; the commit writes them as index rows (`ws_runs`, `remote = 1`, archived so they are never loaded into memory) in the same pass that archives finished runs, so they keep their places in the list, and a run already waiting leaves in the commit that starts it. A run whose next step is a data task leaves after the save has performed it, in a second commit. The host then calls the run object, which creates the run (`execution::start`) and sets its alarm. Starting is idempotent by run ID.
- **Stepping:** the run object's alarm processes the current version (`execution::begin`), performs a claimed HTTP task with the worker's existing HTTP executor, and applies the outcome (`execution::finish`). The alarm is the run's single live message: the latest scheduled time wins.
- **Index reports:** after every save, the run object reports `{version, columns, body}` to the workspace (in the background, so a busy workspace never holds up the next step; the final report is awaited before the run object deletes its storage), which updates the index row in one statement, ignoring reports older than the row's version. Lists, details, MCP reads, admission (`MAX_ACTIVE_RUNS`) and retention read the index, which lags the run by at most one report. The workspace never loads these runs.
- **Control:** pause, resume and cancel on a remote run are forwarded to its run object, which applies them with compare-and-swap and reports.
- **Finishing:** once a run is terminal and its final report is acknowledged, the run object deletes its storage. The index row is then an ordinary archived run, kept by retention.

### Cloudflare Queues scheduling

`RUN_EXECUTION=queues` keeps the run objects as the `RunStore` and makes Cloudflare Queues the `WorkBus`:

- **Messages:** `{workspace, run, version}`, JSON, sent by the run object after every save that leaves work (`WorkBus::send`'s delay becomes the message's delay, rounded up to whole seconds). The run object deletes its alarm while a message carries the run.
- **Consuming:** the consumer (`#[event(queue)]`, batches of up to 10) handles each message by calling the run object's `/begin` with the message version. The run object runs `execution::begin`, saving a claim before any I/O, and returns the claimed task, if any. The consumer performs the HTTP call itself, then calls `/finish`, which applies the outcome with compare-and-swap. A failed message is retried; the version check makes that safe.
- **Parallel requests:** a claim's next message is due at once while another request is ready, so parallel branches' requests are in flight together on separate consumer invocations. Once none is ready, the next message waits for the earliest claimed deadline, which settles a request whose consumer never reported back.
- **Long waits:** Queues delays stop at 24 hours, so a longer wait uses the run object's alarm, which sends the message once it is due within a day.
- **Reports and final deletion** are the same as with alarm scheduling; the alarm retries a final report that failed.

### Run trees

A top-level run and every run it invokes (Execute Workflow and Each children, and theirs) form a **run tree**, and the tree, not the run, is the unit of execution outside the workspace:

- **Same semantics:** `execution` steps a tree by loading it into a scratch `Workspace` holding only the tree's runs, and calling the workspace's own `alarm`, `claim_external`, `complete_external` and `settle_invocations`. Child creation, collection, cancellation, Each concurrency and failure policies, child budgets and simulation clocks are therefore the workspace's code, unchanged. The equivalence harness now publishes every example together and compares every run in a tree, children included, against the workspace.
- **Starting:** the workspace starts a run and settles its invocations, as always, so runs whose children finish at once still answer with their result. If the root is still waiting, the root and its children leave together (once all its children are in memory); their index rows keep their places, children created later are listed just above their root, and the host starts one run object for the tree.
- **Storage:** the root row keeps the tree's version (the compare-and-swap); descendants are rows of a `child` table, in creation order, each with the version that last changed it, so a save rewrites only changed children. Every message, report and control request is per tree, addressed by the root.
- **Reports:** a report carries the root and the children changed since the last accepted report. The index adds a child it doesn't have yet (one created in the run object), listed just above its root, and ignores rows older than it has.
- **Parallelism:** a tree's engine steps run on one object, but its HTTP calls still run concurrently (on consumers with queues), and Each runs at most 4 items at a time, so one object per tree costs little. Trees are independent of each other and of the workspace.
- **Not yet:** children are not refused by `MAX_ACTIVE_RUNS` inside a run object (they count once reported), and a tree's state must fit one object's memory, bounded by the child budgets (16 children, or 256 with Each).

### Data and webhook responses

- **Data operations:** a data task is not claimed. `execution::begin` saves the tree, sends a retry message (`DATA_RETRY_MS`, 10 s) and returns `Step::Data`; the host asks the workspace (`/internal/runs/data`) to apply it with `execution::perform_data` in one SQLite transaction with its receipt, keyed by the task's idempotency key, then reports the outcome with `execution::finish_data`. If the outcome is lost, the retry message performs the task again, and the receipt replays the first result, so an effect is applied once. Receipts of runs active in run objects are kept until they finish. A failed effect is rolled back before its outcome is recorded, as in the workspace. Data tasks are performed straight after the save that created them, so their times match the workspace's.
- **Why not per-dataset objects yet:** data stays in the workspace's SQLite database, where management, MCP and imports already use it transactionally. Each operation is a short transaction, so the workspace is not a bottleneck for run state; moving datasets to their own objects remains possible behind the same call.
- **Simulations with data** stay in the workspace: they run against a snapshot of the live tables taken when they start.
- **Webhook responses:** the handler holding the request (in the workspace) polls the run object (`/reply`) instead of the workspace while the run is there. The run object applies the workspace's `webhook_reply` to its tree (`execution::reply`), saving a submitted response before returning it. A tree is kept while a run in it holds an open response window, and wakes at the window's deadline to close it, as the workspace's alarm does, so a response is never lost to deletion and an abandoned one times out.

### Acceptance targets

- Workspace operations should write only changed rows. Measure save cost at 100, 1,000 and 10,000 entities and move to explicit change tracking.
- Keep at least 200 concurrent runs with 50 KB state cancellable.
- Verify 1,000 concurrent runs in a Cloudflare workspace with median step latency within twice that of a single isolated run.
- Measure webhook acceptance latency against active-run count.
- Exercise duplicate delivery, stale messages, interrupted consumers, run recovery and workspace migrations.

### Measured on workerd

`scripts/scale-check.mjs` starts N runs at once in one workspace (each waits 3 s, then makes an HTTP call), cancels 10 while they wait, and measures each run's *lateness*: when its HTTP step ran, by the upstream's clock, beyond its wait, counted from when its start was acknowledged. CI runs it (300 runs, and 100 with 50 KB inputs) checking only that every run finishes and every cancel applies.

workerd runs every Durable Object of a worker on one isolate, so these figures are one machine's CPU shared by the workspace and every run object; on Cloudflare each run object is a separate isolate. Measured on one CI-class machine:

| Runs | Input | Mode | Single run lateness | Concurrent median / p95 | Start acknowledged (median / p95) |
| --- | --- | --- | --- | --- | --- |
| 200 | small | objects | −0.1 s | −0.1 s / 0.0 s | 0.3 s / 0.4 s |
| 200 | 50 KB | objects | −0.1 s | 1.5 s / 2.3 s | 0.8 s / 2.1 s |
| 1,000 | small | objects | −0.1 s | 0.6 s / 1.2 s | 0.8 s / 1.2 s |
| 1,000 | small | queues | 1.0 s | 0.2 s / 0.6 s | 0.6 s / 1.3 s |
| 200 | 50 KB | queues | 1.0 s | 0.6 s / 1.1 s | 0.6 s / 4.8 s |

(Negative lateness: the step ran before the start's acknowledgement reached the client.)

- **Large state:** 200 concurrently active runs with 50 KB inputs all finish and stay cancellable. Before stage 2, the 6 MB memory budget admitted about 85.
- **1,000 runs:** all finish locally in both modes. Median lateness was 0.6 s with alarms and 0.2 s with queues. The 1,000-run production latency target remains unverified.
- **Not yet measured:** webhook acceptance latency against active-run count.
- **Next bottleneck:** starting runs. Every start goes through the workspace: admission, a commit and the run object start.
- **Cheaper starts:** a start now commits once (a run already waiting leaves in the commit that starts it), records cache their list entry so indexing a run or applying its report never restores its checkpoint, a full view decodes it once, values are measured without encoding a copy, and arming the workspace's alarm only checks whether an unstarted run exists. Started one at a time on workerd, a start's acknowledgement took 20 ms (small input) and 33 ms (50 KB) before, and 16 ms and 23 ms after. Natively, the workspace's CPU per 50 KB start fell from 3.8 ms to 1.9 ms; most of the rest is the engine measuring its state against the checkpoint limit on every step.
- **Regression:** the full `pnpm test:runtime` suite passes unchanged, including storage atomicity, restart persistence and webhook response windows.
- **Change tracking:** every operation in the native suites matches a fresh reload after commit.

## Alternatives considered

- **Raise `MAX_STATE_BYTES`:** quick, but it keeps whole-document cost per operation and makes every request slower as the budget fills. It can't go far past the per-row limit for a single run anyway. Not a fix.
- **Keep the document but write only the records that changed:** this removes the write amplification but not the load cost or the budget. Useful as an interim step (phase 4 without phases 1–3) if the full change is too large at once.

### Resident save-cost baseline (25 November 2026)

Run `cargo run --release --locked -p workflow-service --example workspace_save_cost -- --samples 25` to measure change detection and commits at 100, 1,000 and 10,000 entities. The native benchmark uses separate workflow-heavy and revision-heavy workspaces, excludes fixture construction/load, warms up three times, and reports JSON lines with median/p95 timings, resident bytes and actual writes. Both no-op commits and one-workflow edits are measured; edits assert exactly one upsert. It uses the current memory budget unchanged.

An initial optimization makes `split_meta` serialize only management, audit sequence and legacy webhook-event metadata. Previously it built a JSON value for every collection and then discarded those collections, after the entity split had already encoded them. Metadata bytes/key ordering remain identical; a parity test compares the projection with the persisted Workspace contract, and the SQLite rollback/migration suite covers storage behavior.

Illustrative native release measurements on one development machine, 25 samples (milliseconds; not Cloudflare CPU or end-to-end HTTP latency):

| Resident profile | Entities | Before median commit | After median commit | Speedup |
| --- | ---: | ---: | ---: | ---: |
| workflows | 100 | 0.278 | 0.099 | 2.82× |
| workflows | 1,000 | 2.867 | 0.954 | 3.01× |
| workflows | 10,000 | 29.889 | 9.687 | 3.09× |
| revisions | 100 | 0.263 | 0.104 | 2.53× |
| revisions | 1,000 | 2.647 | 1.053 | 2.51× |
| revisions | 10,000 | 27.243 | 10.636 | 2.56× |

These results do **not** meet the constant-cost target. The remaining entity encoding, digest comparisons, ordering, deletion detection, retention and host workspace cloning still grow with resident state. The next change needs an explicit mutation journal/dirty-record boundary that captures inserts, updates and deletes across all mutation paths, including migration and rollback, rather than assuming SQL upsert count measures save CPU. The benchmark isolates `Tracked::changes` and `Tracked::commit` with MemoryStore; SQL, host cloning, HTTP handling, workspace loading and alarm/webhook acceptance need separate host-level measurements. No timing threshold is asserted in CI.
