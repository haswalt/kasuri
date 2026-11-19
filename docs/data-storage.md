# Persistent workflow data foundations

Status: the runtime-independent `workflow-data` crate, SQLite host and owner management APIs are implemented and tested on native Rust and real workerd. The dashboard supports table/context creation, row editing/deletion with version checks, TTL, filters, keyset pagination and renaming. MCP tools and workflow connector integration are subsequent increments. Data Tables and Context Store are not advertised as available connectors until those paths work end to end, including isolated simulations and importable examples.

Data Tables and Context Store share one storage model. A table holds keyed JSON objects with optional declared field types and required fields; a context namespace holds arbitrary keyed JSON values. Hosts scope stores to the selected workspace and verify membership before access.

The initial primitives cover table creation/name changes, row get/put/delete, insert-if-absent, version-checked writes and filtered queries ordered by key. Record versions come from a monotonic table data revision, so deleting and reinserting a key cannot accidentally satisfy an old version check. Field-schema migrations and secondary indexes remain to be implemented.

TTL is optional. Expired rows are absent from reads and ordinary queries and can be claimed again by a new insert-if-absent operation. Replaying an original successful task receipt returns its original result even after TTL expiry; recovery never repeats that task's mutation. A different business event/run uses a different receipt key. A workflow must branch on the claim result before performing a protected effect.

Hosts execute each mutation and receipt in the same storage transaction. The receipt fingerprint includes the table and command, including TTL/configuration. Reusing a receipt key for a different command is a conflict. The native reference store rolls back on errors; persistent hosts must provide the same boundary using SQLite transactions. Batch storage changes commit together or roll back on errors. Per-row `existing`/`conflict` outcomes are structured results and must be checked by the caller; batching is not a conditional business-rule language.

Queries use bounded keyset pagination, JSON-pointer filters and explicit continuation. An empty result page can still have a continuation cursor when a scan found no matching rows. Result and scan-byte budgets bound memory separately from row count. Current bounds are 32 KiB per JSON value, 100 result rows, 1,000 scanned rows/1 MiB per scan and approximately 60 KiB per query result. These keep node outputs within the engine's existing JSON budget. Live keyset pagination is not a historical snapshot; simulation hosts must pin/copy a source snapshot into an isolated overlay and keep it separate from live storage.

Persistent table rows must live outside workflow/run JSON checkpoints. Engine task receipts must remain recoverable until the completed task checkpoint is durable; time-based cleanup alone is insufficient for a long-running workflow. Run-scoped receipt cleanup and SQLite simulation snapshots are part of the host integration still to come.


## Owner management API

These endpoints use the existing browser ownership session, workspace selection header, installation protection and CSRF checks. Scope is selected by the authenticated host; a request cannot choose an arbitrary storage or receipt namespace.

- `GET /api/tables?limit=25&cursor=...` lists bounded metadata summaries. The cursor is optional.
- `POST /api/data` accepts exactly one `command` or a `commands` array. Mutations and batches require an 8–128 character `idempotencyKey`.
- Responses contain `result` and `replayed`. A matching successful receipt replays the original result; conflicting reuse returns 409. Management receipts remain valid for 24 hours, with incremental physical cleanup. Run-domain receipts have separate identities and do not use time-based expiry.

Create a table explicitly; no datasets are seeded on installation:

```json
{"idempotencyKey":"create-events-v1","command":{"operation":"create_table","definition":{"id":"events","name":"Events","kind":"table","fields":[{"name":"active","kind":"boolean","required":true}]}}}
```

A context namespace uses the same command with `kind: "context"` and no declared fields. Its values can be any JSON type. Table values must be JSON objects.

```json
{"idempotencyKey":"claim-event-123","command":{"operation":"insert_if_absent","tableId":"events","key":"event-123","value":{"active":true},"ttlMs":60000}}
```

The result reports `status: "inserted"` and `inserted: true`, or `status: "existing"` and `inserted: false`. Other commands are `get_table`, `rename_table`, `get`, `put`, `delete` and `query`. `put` is an explicit upsert; supply a row `expectedVersion` for compare-and-set. `delete` also accepts an optional expected row version. Metadata renames require the table's metadata version. Schema alteration and secondary indexes remain pending.

```json
{"command":{"operation":"query","tableId":"events","query":{"limit":25,"filters":[{"pointer":"/active","operator":"eq","value":true}]}}}
```

Use the returned `nextCursor` in the next query. Queries need no idempotency key unless the caller deliberately wants a replayable observed result. SQL row iteration is bounded by bytes and count before values reach the engine or UI. User-controlled values and cursors are parameter bindings, not SQL syntax.

Rows, table definitions and receipts live in dedicated SQLite tables outside workflow/run checkpoints. A mutation, its successful receipt and its management audit entry commit in the same synchronous transaction. Failed batches roll back staged rows, table data revisions and receipts. Audit contains table names, operation names and version references, excluding row keys and values. Replays, reads and unchanged conditional outcomes do not create duplicate audit events. Engine integration and simulation snapshot/receipt lifecycle handling remain pending.
