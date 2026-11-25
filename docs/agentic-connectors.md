# Agentic connector roadmap

Status: selected requirements; not implemented yet. Catalogue entries must remain Planned until executable adapters exist.

| Option | Role in a workflow | Required capabilities |
| --- | --- | --- |
| Cloudflare Workers AI | Native model execution | Model discovery/selection, typed input/output, structured results and usage budgets |
| Cloudflare AI Gateway | Model access through a configured gateway | Connect using operator-owned gateway/provider configuration; secret-backed authentication; supported model requests and tool-call round trips; correlate usage/errors with runs |
| MCP Client | Call a remote MCP server directly | Discover approved tool schemas, invoke tools, handle authentication/session lifecycle, and persist invocation outcomes |
| Cloudflare MCP Portals | Access tools through a Cloudflare-managed MCP portal | Connect to an operator-provided portal endpoint; discover permitted native/aggregated tools; invoke tools within portal and Kasuri policies; support appropriate machine authentication and reconnect behavior |
| AI Agent | Orchestrate model and tool calls | Combine a model/provider or gateway connection with direct MCP/portal tool sources; bounded iterative execution, structured results, explicit permission/approval policies and audit events |

AI Gateway and MCP Portals are separate integration options. AI Gateway fronts model requests; an MCP portal provides an endpoint for curated MCP server capabilities. Neither replaces Kasuri's inbound [MCP management server](agent-mcp.md), which allows external agents to manage Kasuri itself.

## Implementation requirements

- Keep adapters and execution logic in Rust/Wasm, with runtime coverage on Cloudflare.
- Configure gateway/portal endpoints and credential references per connection; keep account IDs, tokens and deployment-specific endpoints out of the public repository.
- Let an agent workflow select its model/gateway and its approved tool sources independently, including native Cloudflare tools exposed through a portal where available.
- Discover actual tool schemas and permissions from the connected portal/server. Do not assume every portal exposes every Cloudflare tool, or bypass its access policies.
- Support noninteractive machine authentication where supported by the selected portal/provider. Treat interactive OAuth setup as a separate connection workflow; never silently fall back to anonymous access.
- Persist tool dispatch/results with correlation IDs and explicit replay/idempotency semantics. Apply timeouts, rate/concurrency limits, iteration/token budgets, secret redaction and per-tool authorization.
- Simulations use explicit model/tool fixtures without calling live providers or tools. Potentially destructive tool calls require the configured approval policy.
- Treat model output, tool descriptions and tool results as untrusted data, and preserve the distinction between tool discovery and permission to execute it.

## References

- [Cloudflare AI Gateway](https://developers.cloudflare.com/ai-gateway/)
- [Cloudflare MCP server portals](https://developers.cloudflare.com/cloudflare-one/access-controls/ai-controls/mcp-portals/)
- [Cloudflare's MCP client overview](https://developers.cloudflare.com/agents/model-context-protocol/apis/client-api/) describes platform capabilities; its TypeScript SDK examples do not change Kasuri's Rust backend requirement.
