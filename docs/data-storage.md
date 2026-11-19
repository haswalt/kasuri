# Persistent workflow data foundations

Status: the runtime-independent `workflow-data` crate is implemented and tested. SQLite hosting, management UI/API and workflow connector integration are subsequent increments. Data Tables and Context Store are not advertised as available connectors until those paths work end to end, including isolated simulations and importable examples.

Data Tables and Context Store share one storage model. A table holds keyed JSON objects with optional declared field types and required fields; a context namespace holds arbitrary keyed JSON values. Hosts scope stores to the selected workspace and verify membership before access.

The initial primitives cover table creation/name changes, row get/put/delete, insert-if-absent, version-checked writes and filtered queries ordered by key. Record versions come from a monotonic table data revision, so deleting and reinserting a key cannot accidentally satisfy an old version check. Field-schema migrations and secondary indexes remain to be implemented.

TTL is optional. Expired rows are absent from reads and ordinary queries and can be claimed again by a new insert-if-absent operation. Replaying an original successful task receipt returns its original result even after TTL expiry; recovery never repeats that task's mutation. A different business event/run uses a different receipt key. A workflow must branch on the claim result before performing a protected effect.

Hosts execute each mutation and receipt in the same storage transaction. The receipt fingerprint includes the table and command, including TTL/configuration. Reusing a receipt key for a different command is a conflict. The native reference store rolls back on errors; persistent hosts must provide the same boundary using SQLite transactions. Batch storage changes commit together or roll back on errors. Per-row `existing`/`conflict` outcomes are structured results and must be checked by the caller; batching is not a conditional business-rule language.

Queries use bounded keyset pagination, JSON-pointer filters and explicit continuation. An empty result page can still have a continuation cursor when a scan found no matching rows. Result and scan-byte budgets bound memory separately from row count. Current bounds are 32 KiB per JSON value, 100 result rows, 1,000 scanned rows/1 MiB per scan and approximately 60 KiB per query result. These keep node outputs within the engine's existing JSON budget. Live keyset pagination is not a historical snapshot; simulation hosts must pin/copy a source snapshot into an isolated overlay and keep it separate from live storage.

Persistent table rows must live outside workflow/run JSON checkpoints. Engine task receipts must remain recoverable until the completed task checkpoint is durable; time-based cleanup alone is insufficient for a long-running workflow. Run-scoped receipt cleanup and SQLite simulation snapshots are part of the host integration still to come.
