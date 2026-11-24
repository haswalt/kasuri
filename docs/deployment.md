# Cloudflare deployment

Deploy from the repository root to your own Cloudflare account. No hosted service, account ID, domain or credentials are provided by this repository.

```sh
pnpm install
pnpm build
pnpm deploy:check
pnpm exec wrangler login
pnpm deploy
```

The build uses the official `worker-build` CLI and `workers-rs` to compile `workflow-worker` to Wasm. Wrangler loads `crates/workflow-worker/build/index.js`, the generated entrypoint. HTTP routing, workspace storage, connector execution and scheduling are Rust. React static assets share the same origin.

`wrangler.jsonc` creates a Worker named `kasuri`, with an `ASSETS` binding, a `WORKSPACES` Durable Object namespace, and a `WorkflowWorkspace` SQLite class migration. It also binds a `kasuri-runs` queue (producer and consumer), used only with `RUN_EXECUTION=queues`; `pnpm deploy` and the deploy button create it if it doesn't exist. Change the Worker name before deploying if necessary. Preserve the namespace/class/migration history when upgrading an installation that contains data. This is a fresh deployment configuration and does not migrate storage from the former TypeScript backend.

Workspaces start empty and persist. An HttpOnly, SameSite=Strict cookie carries a workspace routing ID and an independent session token; only its hash is stored for management authorization. Secure is enabled on HTTPS. Foreign-Origin and cross-site mutation requests are rejected. There is no workspace expiration mode. Run and cron state is saved together in SQLite; alarms resume waits and dispatch scheduled core workflows.

For private deployments, configure Cloudflare Access or set optional Basic credentials:

```sh
pnpm exec wrangler secret put BASIC_AUTH
```

Enter `username:password`. The health endpoint is unauthenticated for probes. Do not store credentials in tracked configuration. For local authentication, put `BASIC_AUTH="username:password"` in ignored `.dev.vars`.

## API

| Method | Path | Behavior |
| --- | --- | --- |
| GET | `/api/health` | Rust Wasm runtime health |
| GET | `/api/workspace` | Workflows, run summaries, connector manifests |
| PUT | `/api/workflows/:id` | Save a draft workflow document |
| POST | `/api/workflows/:id/publish` | Validate and publish, registering cron if present |
| POST | `/api/workflows/:id/pause` | Pause scheduling |
| POST | `/api/workflows/:id/resume` | Resume scheduling |
| POST | `/api/runs` | Start `{ workflowId, mode: "run" or "simulation", input }` |
| GET | `/api/runs/:id` | Inspect a run and node outputs |
| POST | `/api/runs/:id/pause`, `/resume`, `/cancel` | Control a run |

The Rust service bounds JSON bodies to 128 KiB and what a workspace holds in memory to 6 MB (see [concurrent runs](#concurrent-runs)). Finished runs are stored as history outside that budget (see [run history](#run-history)). These are application safeguards independent of Cloudflare account quotas. [Scalable workspace storage and execution](workspace-storage.md) describes the storage design and what comes next.

### Run history

Finished runs are kept in storage and listed with active runs, newest first. By default each workflow keeps its 100 newest finished runs. Configure history as Worker variables on Cloudflare, or in the environment when self-hosting:

| Setting | Default | Meaning |
| --- | --- | --- |
| `RUN_HISTORY` | `100` | Finished runs kept per workflow. `0` or `unlimited` keeps any number. |
| `RUN_HISTORY_DAYS` | none | Delete finished runs created more than this many days ago. |
| `FAILED_RUN_HISTORY_DAYS` | `RUN_HISTORY_DAYS` | The same for failed runs, so they can be kept longer. |

Older runs are deleted when a commit archives runs of that workflow, and the age limits are checked on every commit, using indexes. Runs that are still active, or that an active run depends on, are never deleted. An invalid value fails requests with a configuration error rather than keeping a different amount of history. Without any limit, history is bounded only by the Durable Object's 10 GB storage.

### Run execution

`RUN_EXECUTION` chooses where runs execute:

| Value | Meaning |
| --- | --- |
| empty or `workspace` (default) | Every run executes in its workspace's Durable Object. |
| `objects` (preview) | A run that still has to wait after its first step (on an HTTP call or a timer) moves to its own Durable Object, which steps it on its own alarm and reports to the workspace. The workspace keeps only an index entry, so waiting runs don't use its memory, and runs no longer share one object's thread. |
| `queues` (preview) | As `objects`, but each step is a message on the `kasuri-runs` queue, and HTTP calls are made by the queue consumer rather than inside the run object. Consumers scale out with the queue backlog, so many runs' calls proceed in parallel. Waits longer than a day are kept on the run object's alarm until they are due within a day. |

`queues` uses Cloudflare Queues, which the Workers Free plan includes with a daily operation limit (about three operations per step: send, read and acknowledge). A run's steps are delivered at least once; the run object's versioned compare-and-swap makes duplicate and late messages harmless, and an HTTP request is claimed before it is sent, so it is sent once (with its idempotency key) unless a consumer fails mid-call. On Cloudflare you can switch between `objects` and `queues` at any time: a run in a run object continues in the mode its next step finds. When self-hosting, let queued runs finish before switching away from `queues`, because the other modes start workerd without the queue.

A run moves to a run object once it is waiting (on HTTP, a timer, a child or its webhook response), after its first steps and any data operations due have run in the workspace, so runs that finish at once, or on their data operations, still answer with their result. A run that invokes other workflows (Execute Workflow or Each) moves with its child runs: they execute together in its run object, and appear in run lists as they are created. Data Tables and Context Store operations from a run object are applied to the workspace's data in a transaction, keyed so that a retry replays the first result rather than applying it twice. A webhook request waiting for Respond to Webhook is answered from the run object. Only simulations that use data operations (directly or in a workflow they invoke) stay in the workspace, with the data snapshot they started with. Lists and details of a run in a run object can lag it by one step. Pause, resume and cancel are forwarded to the run object, a child's through its root. The run object deletes its storage once every run in it has finished (background children included) and the workspace has their final state. Children created in a run object count toward `MAX_ACTIVE_RUNS` for new starts, but are not refused by it; each run's children stay bounded as in the workspace (16, or 256 with Each, which runs at most 4 items at a time). See [run objects](workspace-storage.md#first-deployment-run-objects-b1) for the design.

### Concurrent runs

Runs are not limited by count by default. A workspace holds its workflows, revisions, active runs and audit entries in memory, within a 6 MB budget; finished runs leave memory and don't count. The budget protects the Durable Object's 128 MB memory limit: measured on workerd, a request uses about ten times the workspace's in-memory state. Active runs can never be pruned, so a workspace stops starting new runs, Execute Workflow children and Each items once that state reaches 75% of the budget. Run state stores each repeated value once (an input passed through a trigger and a Wait node is kept once, not three times), so that is about 85 concurrent runs with 50 KB inputs, or many more with small ones. The remaining budget keeps the workspace writable while active runs finish; refused starts return `429`, and Each items wait and retry. To cap concurrency as well, set `MAX_ACTIVE_RUNS` to a positive integer (as a Worker variable on Cloudflare, or in the environment when self-hosting). Empty, `0` or `unlimited` mean no count limit. Running and paused runs, and runs holding an open webhook response, count as active. An invalid value fails requests with a configuration error rather than silently using another limit. See Rust source for graph/engine budgets.

### Workspace storage

Each workspace is stored in its Durable Object's SQLite database as one row per workflow, revision, run, audit entry and webhook, with indexed run columns. A save writes only the rows that changed. The layout is described in [workspace storage](workspace-storage.md).

Workspaces saved by earlier releases, as whole-workspace record generations or as a single row, migrate to this layout the first time they're loaded after upgrading, in one transaction. If the migration fails, it rolls back, the old layout is left as it was, the error is logged, and the next load tries again.

After migrating, a workspace can't be read by releases before table storage. To downgrade, use the release that introduced table storage with `WORKSPACE_STORAGE=generations`, which migrates workspaces back. `WORKSPACE_STORAGE` is no longer read.

Scheduling uses five-field cron expressions and IANA timezones. Missed occurrences coalesce to one run after downtime. Pausing a workflow removes its next cron occurrence; pausing a run removes its wait alarm until resumed. The current core connectors have no external side effects.

```sh
pnpm test
pnpm deploy:check
```

Local runtime checks exercise the actual Rust Wasm Worker in workerd, including authentication, simulations, wait recovery after restart, schedule persistence and cross-site protection. No Cloudflare account is needed for these checks.

## HTTP egress

HTTP Request allows public HTTPS by default. Optionally restrict it with `HTTP_ALLOWED_HOSTS`; empty means unrestricted public destinations. Basic passwords and sensitive custom headers use named aliases in the `HTTP_SECRETS` Worker secret. See [connector configuration, simulation and recovery semantics](connectors.md).

## Webhook triggers

Generated webhook endpoints are served by the same Worker under `/hooks/<workspace-id>/<hook-id>` (with earlier `/api/hooks/...` URLs retained as compatibility aliases), authenticated independently of browser management access. No new bindings or Durable Object class migrations are required. See [webhook setup, lifecycle and session migration](webhooks.md).
