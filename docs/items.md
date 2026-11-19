# Item processing

Arrays remain ordinary single JSON values unless a workflow explicitly adds collection processing. Three nodes provide the initial item-processing set:

- **Split / Batch** selects an array and divides it into ordered arrays of a chosen size. Output is `{batches, total, batchSize}`; it does not execute downstream nodes once per item automatically.
- **Each / Loop** selects an array and invokes a pinned reusable workflow for each item. Each child receives one original item unchanged. Select an active published workflow with Execute Workflow Trigger; use its input/output schemas to define the item contract.
- **Aggregate** selects an array and collects a field, counts entries, or sums numeric values. Select a field or the whole item; choose whether a missing field fails or is skipped. Null and non-numeric values are not silently converted to numbers. All-integer sums preserve 64-bit integer precision; sums containing fractional JSON numbers use finite floating-point arithmetic.

## Each / Loop

Each assigns an identity derived from the parent run, node and original item index. Repeated values still have distinct identities. Results stay in original input order, regardless of completion order. The selected child definition and its reachable definitions are pinned before the root run starts; later publication changes do not affect later items in that run.

Output is `{items, total, succeeded, failed}`. Each item record includes `index`, `itemId`, `runId`, `status`, and either `output` or a safe `error` code. Contract-rejected items may have no child run. Detailed run views retain item progress and results, with links to retained child runs. In child workflows, Request Context metadata exposes `parent.itemIndex` and `parent.itemId`; business input remains the item itself.

Select concurrency **1**, **2** or **4**. The existing workspace active-run budget also applies, so items queue when other runs consume capacity. If the selected child itself invokes workflows or contains Each, the outer collection runs one item at a time to reserve capacity for nested work. This is bounded child execution, not an unbounded thread pool.

**Stop and cancel remaining items** fails the collection on an item error and stops launching items. **Collect errors and continue** records failed items and processes the rest. To collect only successful results, use Aggregate on `/items`, select `/output`, and choose **Skip missing values**. To sum a field returned by the child, select a path such as `/output/item/amount`.

Configure retries on the child workflow's individual nodes. A retry belongs to that item's persisted child run, preserving its stable effect identity and successful preceding steps; completed sibling items are never restarted. Whole-collection retries are rejected rather than replaying successful items. The editor keeps the collection deadline editable and directs retry configuration to the child nodes.

Pause retains item progress and prevents new item launches; already-started children can finish. Resume collects those results and continues. Cancellation stops the parent collection and cancels its active awaited children. Already-committed external/data effects cannot be undone. Large collections yield to the shared durable alarm scheduler instead of completing unbounded synchronous work in one request.

Simulation always creates simulated children. HTTP nodes use simulation fixtures, and each child has its own isolated data snapshot. These snapshots are independent per child, not one shared mutable collection transaction. For cross-item concurrency/deduplication behavior, test a real workflow against synthetic keys; simulation cannot prove shared-write races.

## Bounds and recovery

Each supports at most 100 input items, 32 KiB of combined retained item outputs, and the normal node/checkpoint/value limits. Return compact summaries or durable data references for large per-item results. An oversized result becomes a bounded item error; no output is silently truncated. The node's timeout bounds the whole collection, while child nodes retain their own deadlines and retry policies.

Batch supports up to 1,000 input entries with batch sizes 1–100. Aggregate supports up to 1,000 entries and the normal 64 KiB node output bound. Empty collections are explicit: zero batches, zero children, empty collected output, or a numeric count/sum of zero.

Root trees using Each have a cumulative budget of 256 child runs and retain the existing depth-four bound. Normal invocation-only trees retain their previous sixteen-child budget. Completed item results are durably copied into the parent before their child records become eligible for normal bounded retention; a pruned child detail may be unavailable while the parent's collected item result remains. Stored older runs have empty item state by default and keep their original behavior.

Data effects, dispatch markers, item progress and completion checkpoints use the existing Rust persistence/receipt boundaries. MCP execution and detailed reads inspect the full pinned child graph set for explicit dataset scopes. Each cannot hide dataset access behind an apparently data-free parent.

## Examples

[22 Batch and aggregate](../examples/workflows/22-batch-aggregate.json) needs only import and `{}`. Import and publish [23 Item worker](../examples/workflows/23-item-worker.json) first, then import [24 Each items](../examples/workflows/24-each-items.json) or [25 Collected item errors](../examples/workflows/25-collect-item-errors.json). Dashboard import automatically binds their unpinned references to the unique imported child. All fixtures are synthetic, require no secrets and simulate offline.

A batching pattern is `source array → Split / Batch → Each / Loop (array /batches) → Aggregate`. The reusable child receives a batch array; its contract must accept an array. A simple per-item pattern omits the batch adapter and passes each item directly.
