# HTTP management API v2

The Rust/Wasm entrypoint serves the API on Cloudflare. `/api/health` reports version `2`; `/api/workspace` reports `apiVersion: 2` and `editPreconditionsRequired: true` alongside the selected workspace's workflows, runs and connector manifests.

Browser ownership uses an HttpOnly session cookie. The route selects a workspace per tab; management requests carry its key in `X-Kasuri-Workspace` (`personal` or an owned workspace ID). The server verifies ownership before routing. A workspace key is not an authorization credential. Non-browser agent integrations should use the scoped [MCP server](mcp.md).

## Required edit preconditions

This is an explicit migration from API v1: unversioned writes to existing workflow graphs, publication/activation changes and workflow execution requests are no longer accepted. There is no compatibility header or fallback that bypasses the check.

1. Read `/api/workspace` and select the workflow to inspect. Its `version` is the saved edit version; `revision` counts publications.
2. To replace an existing draft, `PUT /api/workflows/<id>` with the complete workflow document including that `version`.
3. For the following operations, include `expectedVersion` in the JSON body:

| Request | Body |
| --- | --- |
| `POST /api/workflows/<id>/publish` | `{"expectedVersion": 3}` |
| `POST /api/workflows/<id>/pause` | `{"expectedVersion": 3}` |
| `POST /api/workflows/<id>/resume` | `{"expectedVersion": 3}` |
| `POST /api/workflows/<id>/archive` | `{"expectedVersion": 3}` |
| `POST /api/workflows/<id>/restore` | `{"expectedVersion": 3}` |
| `DELETE /api/workflows/<id>/trash` | `{"expectedVersion": 3}` |
| `POST /api/workflows/<id>/revisions/<revision>/restore` | `{"expectedVersion": 3}` |
| `POST /api/runs` | `{"workflowId":"flow","mode":"simulation","input":{},"expectedVersion":3}` |

The example number must be replaced with the version actually read. Mutations that update a workflow return its current edit version. No-op saves, unchanged publication and repeated activation commands retain the current version. Keep using the returned version for the next operation.

A missing edit precondition returns **428**. A precondition of the wrong type returns **400**; versions must be non-negative JSON integers. A stale version returns **409**. After a conflict, fetch and review the latest workflow before resubmitting; do not silently replace the version on an old graph and overwrite another client's edits.

Creating a new workflow ID does not require a version. The server starts its edit version at 1, clears imported publication state and creates an inactive draft. Scheduled and webhook runs originate from registered triggers, rather than client-supplied `schedule` or `webhook` run modes. Run-control commands operate on an existing run ID and do not replace workflow definitions. Workspace catalogue and credential administration are separate owner operations.

## Draft, publication and trash

`status` describes activation (`draft`, `active`, `paused`). `lifecycle.publishedRevision` identifies the immutable live definition, `lifecycle.enabled` controls future triggers, and `hasUnpublishedChanges` identifies saved draft differences. A draft save preserves the live graph, authentication and schedule. Publishing unchanged content does not create a revision or reactivate a paused workflow.

`GET /api/workflows/<id>/revisions` returns retained immutable definitions. Restore copies one into the draft; explicit publication then creates a new monotonic revision. Legacy baselines are marked, and unavailable historical definitions are not reconstructed.

Archive moves a workflow to recoverable trash and blocks new triggers. The dashboard hides the workflow and its runs. `GET /api/workflows/trash` lists archived workflows. Restoring makes their runs visible again and leaves triggers paused.

Permanent deletion requires the workflow to be in trash and returns `{"deleted":true}`. It removes the saved workflow, publication history and webhook registration. Existing run records follow the workspace retention policy and remain available by run ID.
