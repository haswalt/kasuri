# Calling another workflow

A reusable workflow starts with **Execute Workflow Trigger**. Choose a terminal node as its output, optionally set input/output JSON schemas, then publish and activate it. Schemas use the same bounded offline profile as Validate Schema. In the caller, add **Execute Workflow**, select the reusable workflow and either its current published revision or a retained revision. The editor lists active workflows whose published graph has the required trigger, including when their drafts differ.

Choose **Wait for child result** to receive `{runId, workflowId, revision, status, output}`. A child error, cancellation, rejected contract, missing output or invocation deadline fails the invocation node with `invocation_failed`; its error output can handle that result. Choose **Continue with child run ID** to receive `{runId, workflowId, revision, status}` immediately after durable creation. Background children continue independently, including after the parent finishes or is cancelled. Neither mode calls public webhooks or uses HTTP credentials.

The optional input selector passes the whole current input or the value at a JSON Pointer. The child trigger passes that input unchanged. A selected output node must be terminal; if it is skipped, the child contract fails. JSON schemas validate without transforming values. Input-contract rejection prevents child creation; output-contract rejection marks the child failed. Top-level manual/simulation inspection of a reusable graph remains available, but invocation contracts apply at its invocation boundary.

## Persistence and security

The root run snapshots the complete reachable published graph set before execution. “Current revision” means current at root-run creation, not whenever a delayed invocation executes. Children and grandchildren inherit those pinned definitions; subsequent edits, publication, pause or trash changes do not rewrite an already-started execution. New root runs reject unavailable, inactive or unpublished targets. A revision selection may reference a retained published revision of an active target.

Parent/child links include node IDs, child run IDs, revisions, mode and status; run details link to related runs. A child ID is derived from the parent node's stable effect identity. Retrying/restoring an invocation attaches to that same child and never starts another child for the same node. Children and parent checkpoints share the workspace's synchronous persistence transaction before external effects dispatch. Awaited children remain retained while their parent is active or paused.

Awaited children are cancelled when their parent fails/cancels or the invocation's node deadline expires. Parent pause stops advancing the parent; it does not pause a child already started. A child can finish while the parent is paused; resume collects that result. Background work is independently controllable from its run detail page. Cancellation cannot undo an HTTP side effect already sent.

Simulations create simulated children only. Their clocks advance virtually, their HTTP responses use fixtures, and their dataset writes use run-owned isolated snapshots. Each child is a separate simulation run with its own dataset snapshot; simulation overlays are not a shared parent/child transaction. Simulation time cannot expire unrelated live invocations.

All invocation is workspace-local. MCP execution, publication, resume and detailed reads inspect the complete pinned/reachable graph set for dataset permissions, so a parent cannot hide a child's `data_read`/`data_write` requirements. Old credentials do not gain access by calling a child. There is no cross-workspace workflow-ID lookup.

Initial bounds: depth four, sixteen child runs per root tree (a persisted cumulative budget), sixteen distinct target selections, the existing five active-run workspace limit, and existing graph/checkpoint/value budgets. Recursive invocation is rejected before creating the root run. The invoking node's timeout bounds awaited completion; background runs use their own normal run/node deadlines. These are bounded DAG invocations, not loops.

## Examples

[Example 18](../examples/workflows/18-reusable-workflow.json) simulates independently with `{}`. Import and publish it first; [example 19](../examples/workflows/19-invoke-workflow.json) then demonstrates an awaited result followed by background work. Both contain only synthetic data and need no secrets, URLs or configuration edits. Importing a caller cannot automatically activate its dependencies; publication is an explicit normal dashboard action. The scenario manifest declares the dependency so native and runtime checks can prepare it consistently.
