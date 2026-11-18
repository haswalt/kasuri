# Workflow engine foundation

The Rust workspace contains four runtime-independent crates:

| Crate | Responsibility |
| --- | --- |
| `workflow-schema` | Version 1 JSON graph, node versions, retry policies, typed output ports, DAG validation and graph budgets |
| `connector-sdk` | Serializable tasks/outcomes, safe failure codes, node manifests and the synchronous connector trait |
| `connectors-core` | Declarative trigger input, mapping, equality conditions, merge, delay, JSON parse/stringify and error handling |
| `workflow-engine` | Deterministic scheduling, branch skipping, joins, retry/timeout state, pause/cancel, execution budgets and checkpoint recovery |

`workflow-engin`, the original placeholder crate, has been replaced by `workflow-engine`.

## Run the example

```sh
cargo run -p workflow-engine --example run -- examples/basic-workflow.json examples/input.json
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
```

The example executes real core-node logic using a **virtual clock**. It maps the event, chooses the urgent branch, schedules a one-second wait and collects the result. It serializes/restores the run between transitions to exercise checkpoint recovery. It does not persist to disk, sleep for real, send HTTP requests or connect to Cloudflare.

The engine and its dependencies can be checked for Wasm independently of this native example:

```sh
rustup target add wasm32-unknown-unknown
cargo check --workspace --target wasm32-unknown-unknown
```

This checks compilation; it is not a deployed Worker or a JavaScript/Wasm binding. Platform quotas and the durable hosting design still need a deployment spike. Cloudflare's [Rust Workers documentation](https://developers.cloudflare.com/workers/languages/rust/) describes the platform adapter; its [Workflows guide](https://developers.cloudflare.com/workflows/get-started/guide/) describes the managed execution API. Those are separate from this scheduling library.

## Host protocol

1. Authenticate the trigger, validate its payload and choose a globally unique run ID with inbound deduplication.
2. Create a `Run` with an immutable workflow snapshot, bounded input, clock time in milliseconds and `Limits`.
3. Call `next(now_ms)`. It returns one task, a wake-up time, paused status or terminal status.
4. **Durably persist `checkpoint()` before dispatching a returned task.** Serialize all mutations for a run (for example through a Durable Object). The library has no locking, distributed lease or compare-and-swap store.
5. Dispatch the task to a trusted connector adapter. Apply the task deadline to the underlying request and use its stable `idempotency_key` for supported external writes. Never invoke arbitrary user code in the executor.
6. Call `complete(task.id, outcome, now_ms)` and persist the new checkpoint before requesting more work. Report sanitized structured output or a failure code, never raw errors containing credentials.
7. For `SleepUntil`, schedule a durable wake-up, then call `next` with the host clock. Waiting nodes complete without invoking their adapter again. Retry states produce a new attempt after their backoff. No thread sleeps inside the engine.
8. After process loss, restore the last trusted checkpoint with `Run::restore`. An in-flight task is returned again with the same task ID, attempt and idempotency key, until its deadline expires. Completed nodes are not dispatched again.

The host must use `Run::restore`, rather than directly deserializing a `Run`, to enforce checkpoint validation. Checkpoints are host-owned data and must not be accepted from browser uploads; they are not signed or encrypted by the engine. Store them behind authentication and authorization. Outputs and inputs may contain private data, so production hosts need redaction, encryption, retention policy and artifact storage.

Persist-before-dispatch closes one failure window but does **not** guarantee exactly-once external side effects. A host can crash after an external action commits and before recording its result. Re-delivery then requires connector-level idempotency or reconciliation. Never treat an idempotency key as proof that a remote API supports it. Cancel and timeout stop scheduling; the host must abort an in-flight request when possible, and an already committed side effect cannot be undone.

## Graph and data semantics

- Graph JSON matches editor exports (`schemaVersion`, `type`, `config`, stable node/edge IDs). Missing node `version` defaults to 1, missing retry policy defaults to one attempt, and missing edge `sourcePort` defaults to `success`. UI-only fields such as `status` and `updatedAt` are ignored by Rust. The workflow revision is captured in the checkpoint.
- Exactly one trigger is required. Triggers must have no incoming edges. Duplicate IDs/connections, dangling edges, disconnected nodes, unsupported ports and cycles are rejected before execution. Root and node order in the JSON need not be topological.
- The engine serializes execution: one in-flight task per run. Independent branches are supported but do not execute concurrently in this first implementation.
- A single-parent node receives that parent's selected output directly. A node with multiple incoming edges waits for every parent to settle and receives an object keyed by **active parent ID**. Inactive parents are omitted. If no parent edge is active, the node is skipped, propagating through the untaken branch.
- Normal nodes emit `success`; conditions emit `true` or `false`. All nodes can route a terminal failure through an explicit `error` edge. Retryable failures retry first. An exhausted or permanent failure without an error edge fails the run immediately. The error output contains only `{ "error": { "code": "external" } }` (or another stable code).
- `Error Handler` is a passthrough destination for an error edge; it does not invent recovery behavior or invoke an external alert.
- `Manual Trigger`, `Webhook` and `Schedule` pass through a host-supplied event. They do not register endpoints, verify webhook signatures or register cron jobs.
- `Merge` passes through the input assembled by the scheduler; it does not aggregate arbitrary items or implement streaming.

## Node configuration

| Type | Config |
| --- | --- |
| Manual Trigger / Webhook / Schedule | No execution configuration; the host supplies an authorized event |
| Map / Transform | `mapping`: JSON object or JSON-encoded object. Values are literals, except `{ "$input": "/customer/name" }`, which copies a value using JSON Pointer |
| IF / Condition | `pointer`: JSON Pointer; `equals`: JSON value tested for strict equality |
| Wait / Delay | `delayMs`: nonnegative integer or decimal string |
| JSON Utilities | `operation`: `parse` or `stringify` |
| Merge / Error Handler | No configuration |

Missing input paths fail with `missing_input`; explicit JSON null remains a value. Empty JSON Pointer selects the whole input, and escaped segments such as `/a~1b` work. This is intentionally a bounded reference mechanism; expression evaluation, `{{ ... }}` templates and arbitrary code are not implemented. Node manifests expose configuration metadata for future schema-driven forms; they do not yet generate React forms or replace execution-time checks.

Unsupported types (including HTTP, SaaS and Code) fail explicitly through `unsupported_node`. The SDK provides the task/result boundary for their future adapters; their presence in the frontend catalogue does not indicate implementation.

## Limits and lifecycle

- Hard graph limits: 100 nodes, 1 MiB graph JSON, 64 KiB per node configuration.
- Hard execution data limits: 64 KiB input/output/task payload, 2 MiB serialized checkpoint. Large files/results belong in R2 with bounded references. Invalid or oversized outcomes return an error atomically, leaving the task in flight; the host must report a safe failure or cancel instead of retrying the invalid payload indefinitely.
- Default run limits: 1,000 task dispatches (including retries), seven days elapsed time. These are **application defaults**, not Cloudflare quotas. Host callers can choose tighter limits. Checkpoint-size errors also leave the prior state intact.
- Node `timeoutMs`: positive milliseconds, defaults to 30 seconds. Tasks expose `deadlineMs`, capped at the run deadline. Ticking an expired task applies a retryable timeout. A late success is treated as timeout, not success. Hosts must schedule timeout ticks and bound actual connector requests.
- Node `retry`: `maxAttempts` (1–10, including the first attempt), `delayMs`, `maxDelayMs`. Backoff is capped exponential and deterministic; jitter, Retry-After and connector-wide rate limiting are not implemented yet.
- `pause` blocks new dispatches but accepts the current task's completion. `resume` continues a paused run. Elapsed deadlines include pauses and waits. `cancel` is terminal and rejects late results. Terminal runs cannot resume or replay.
- Times come from the host and cannot move backwards. Identical state, outcomes and clock inputs produce deterministic scheduling. Task identity changes per retry; the side-effect key remains stable for the run/node. Different intentional side effects within a future multi-operation node need operation-specific key extensions.
- Stale and duplicate completions return errors without changing state. The host may acknowledge an already recorded duplicate after consulting its durable task journal.

## Rust Wasm host

`workflow-service` owns persistent workspace documents, simulations and cron registration. `workflow-worker` implements HTTP, SQLite Durable Objects and alarms directly with workers-rs. The official worker-build tool generates the Wasm module entrypoint. Cloudflare and self-hosted workerd execute the same Rust backend. See [deployment](deployment.md) and [self-hosting](self-hosting.md).
