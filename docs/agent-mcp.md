# Agent management through MCP

Status: initial implementation available. See [MCP connection and authorization setup](mcp.md).

The Rust endpoint now implements scoped bearer credentials, OAuth discovery/DCR/PKCE with owner consent and rotating refresh tokens, connector discovery, workflow list/get/create/update/clone/validate/publish/pause/resume, run list/get/simulate/start/control, and bounded audit reads. The portal issues/revokes credentials and displays audit records. Mutation receipts and edit-version checks protect retries and concurrent edits.

The requirements below remain the longer-term acceptance target. Historical revision storage/diff, detailed execution events/timings, per-resource grants, generalized connection-vault configuration and MCP resources are not implemented or advertised. Audit retention is currently 1,000 records, with gap metadata. Protocol support is the 2025 Streamable HTTP family, not the 2026 per-request protocol.

Kasuri must expose an MCP server so agents can discover, build, validate, manage and audit workflows and their runs. This is an inbound management interface for agents connecting to Kasuri. The planned MCP Client connector serves a separate purpose: allowing a workflow to call tools on another MCP server. Cloudflare AI Gateway and Cloudflare MCP Portals are also selected [agentic connector options](agentic-connectors.md).

## Architecture and access

- Expose a remote HTTP MCP endpoint from the Rust Wasm backend, available on Cloudflare deployments.
- Reuse the workflow schema, validation, application services, engine and connector manifests used by the UI and HTTP API. Do not add a TypeScript backend or a separate execution engine.
- Authenticate agents with revocable, scoped credentials and an explicit workspace identity. Browser-session cookies and possession of an MCP session identifier must not grant agent management access.
- Enforce permissions on every tool/resource call. Support separate read/audit, draft-edit, publish/schedule, run-execution and run-control permissions, scoped to the appropriate workspace and resources.
- Provide connection/setup instructions for agent clients, credential rotation/revocation, and deployment configuration without embedding account details or secrets in the public repository.

Tool names below are proposed; the capabilities are requirements.

## Management capabilities

| Area | Required capabilities | Proposed tools |
| --- | --- | --- |
| Discovery | Discover supported connector versions, operations, configuration schemas, ports and implementation status | `connectors.list`, `connectors.get` |
| Workflow inspection | List/search workflows and retrieve graphs, publication state, revisions and schedule information | `workflows.list`, `workflows.get`, `workflows.revisions` |
| Workflow building | Create drafts, edit nodes/edges/configuration, clone and import/export versioned workflow JSON; preserve canvas positions and stable IDs | `workflows.create`, `workflows.update`, `workflows.clone`, `workflows.export` |
| Validation | Validate a proposed graph without saving or executing it; return structured, node-specific errors and configuration requirements | `workflows.validate` |
| Publication | Publish a specific revision, pause/resume scheduling and inspect the resulting registrations | `workflows.publish`, `workflows.pause`, `workflows.resume` |
| Execution | Simulate with explicit fixtures or start a real run of an identified workflow revision | `runs.simulate`, `runs.start` |
| Run inspection | List/filter runs; inspect status, node outcomes, attempts, waits, timings and redacted inputs/outputs/errors | `runs.list`, `runs.get`, `runs.events` |
| Run control | Pause, resume or cancel a run, reporting the effect on pending and in-flight work | `runs.pause`, `runs.resume`, `runs.cancel` |
| Auditing | Query attributable management actions and execution events; compare revisions and export redacted audit evidence | `audit.list`, `audit.get`, `workflows.diff` |

Expose read-only MCP resources for connector schemas, workflow revisions, run summaries and audit records where useful. Lists and audit queries must support bounded pagination and filters; agents must not need to download a whole workspace or poll unbounded histories.

Workflow deletion, rollback and replay can be added when their underlying application semantics exist. MCP must not advertise tools for unsupported operations or planned connectors as if they work today.

## Execution and audit requirements

- Distinguish draft editing, validation, simulation, publication and real execution explicitly. Editing or validating a graph must not publish it, register schedules or call external services.
- Simulations retain the existing no-network semantics and require explicit fixtures for external connectors.
- Use revision checks for edits and expected revisions for publish/run requests. Return a structured conflict instead of overwriting another author’s changes or silently executing a different revision.
- Support idempotency for mutating management calls so client reconnects/retries do not accidentally create duplicate workflows, runs or publication actions. Report uncertain outcomes and existing results consistently.
- Persist an attributable audit trail containing agent/client identity, workspace, action, affected workflow/run/revision, timestamp, outcome and correlation/request ID. Record relevant before/after revision references without copying secrets into audit records.
- Preserve the execution engine’s retry, checkpoint, cancellation and external-effect semantics. Cancellation cannot undo an external write already committed; MCP must not imply exactly-once external execution.
- Keep audit records append-only through the agent interface, enforce access/retention policies, and report gaps or retention limits when exporting evidence. Agent permissions must not permit rewriting their own audit trail.

## Security and agent interaction

- Tool schemas must expose stable IDs, structured results/errors and whether an operation reads state, mutates it or can cause external side effects. Sensitive actions must provide enough context for the connecting client’s approval policy.
- Enforce server-side authorization even if a client suppresses its own confirmation UI. Publish/schedule changes and real execution require their corresponding explicit scopes; read/audit access must never imply these permissions.
- Expose credential aliases and availability metadata only when authorized. Never return secret values, resolved authentication headers, raw credential-bearing checkpoints or unredacted upstream error payloads. Do not provide a general secret-reading tool.
- Treat workflow descriptions, connector output and upstream payloads as untrusted data. They must not expand permissions, change tool policy or become executable instructions inside the server.
- Apply workspace isolation, request/output limits, rate/concurrency budgets and log redaction to MCP just as to other API entrypoints. Propagate correlation IDs through management, execution and audit events.

## Acceptance scenarios

1. An agent connects to a chosen workspace, discovers implemented connector schemas, builds a draft graph and fixes validation errors without triggering external actions.
2. The agent simulates an HTTP → Filter → Sort workflow using fixtures, then an independently authorized execution call starts the selected revision with real input.
3. An agent with read/audit scope traces a failed node, its attempts and related workflow revision, and exports redacted evidence; all mutation attempts are rejected.
4. Concurrent UI and agent edits produce a revision conflict rather than losing changes. A repeated run-start request with the same idempotency key returns the original run.
5. Pause/cancel operations remain effective while external requests are outstanding, and late results cannot overwrite the requested state.
6. Cross-workspace calls, revoked credentials and insufficient scopes are rejected. Shared installation secrets and browser-session capabilities are not exposed through discovery, resources or audit exports.
7. The same interface and permission rules work on Cloudflare, with connection instructions and local conformance tests.

## Implemented workflow construction contract

Agents can now use `workflows_schema_get`, `workflows_examples_list`, `workflows_examples_get` and `platform_capabilities_get` before building. The graph contract is generated from the Rust engine types, shares connector manifests with executable validation, and is embedded in workflow create/update/validate tool schemas. Validation reports stable codes, JSON Pointer paths, expected values/types when available and node/connector context. Public examples are compiled from the repository's tested fixtures, including reusable parent/child workflows, and never seed an installation. `connectors_list` adds public usage configurations and input ports. See [the agent build loop and diagnostics](mcp.md#building-workflows-without-the-visual-editor).

Contract acceptance includes an agent discovering schemas without workspace exports, correcting malformed labels/types/edges, validating and creating a draft, simulating and inspecting results, and creating/publishing a discovered child before simulating its caller. Native tests compile every tool schema and validate all public examples; Wasm HTTP checks exercise the same loop with reader-scope denial for mutations. Complete configuration-specific input/output schemas and higher-level graph mutation helpers remain separate follow-up work; examples and capability guidance describe those runtime contracts today.
