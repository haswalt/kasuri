# MCP Client

Use **MCP Client** in a workflow to list a remote server's approved tools or call a specific approved tool. The server endpoint, credentials and allowed tools are configured by the deployment operator. The node contains a connection alias and a tool name. The workflow editor gets only connection names, approved tools, and setup status for the active workspace. Open **Connections → MCP servers** for the same view outside the editor.

## Configure a connection

Run `pnpm exec wrangler secret put MCP_CONNECTIONS` and enter a JSON object like this, with your installation's values. Get the workspace ID from the segment following `/mcp/` in its MCP management endpoint.

```json
{
  "availability": {
    "url": "https://tools.example.com/mcp",
    "workspaceIds": ["your-workspace-durable-object-id"],
    "allowedTools": ["check_availability"],
    "bearerToken": "your-server-issued-token"
  }
}
```

Omit `bearerToken` for a server that accepts anonymous calls. Configure a least-privilege token on the remote server. Put the token in `bearerToken` and keep credentials out of the URL. Each live connection requires explicit workspaces and tool names. The URL must use HTTPS and follows Kasuri's `HTTP_ALLOWED_HOSTS` and `HTTP_PRIVATE_HOSTS` rules. The client sends credentials only to the configured URL and rejects redirects.

For workspace connections backed by Cloudflare One, see [Cloudflare MCP Portals](mcp-portals.md) to use Access service token bindings instead of a bearer token.

## Use it in a workflow

- **List approved tools** returns `tools` with remote input and output schemas for approved names. `truncated` indicates that discovery reached its scan limit before finding every approved tool.
- **Call a tool** sends the selected JSON object as arguments. Use input bindings to assemble that object, or choose an object from the incoming data. The result includes `tool`, `content`, and `structuredContent` (null if unavailable). Treat everything from the server as untrusted data. Returned resource links are left as data.
- **Simulate** uses the config's `simulationResponse` result object without accessing the connection or calling the server. Try the [synthetic booking lookup](../examples/workflows/36-mcp-client-booking-availability.json) and [tool discovery](../examples/workflows/37-mcp-client-list-tools.json).

The connector uses Streamable HTTP. It negotiates a protocol version for each run step, sends the initialization notification, forwards an assigned session ID, and handles JSON or SSE replies to its POSTs. It attempts to close the session after the operation. It supports servers using MCP protocol versions 2025-11-25, 2025-06-18, or 2025-03-26. Interactive OAuth, legacy HTTP+SSE endpoints, resumable streams, and server-initiated callbacks are outside this version.

Tool errors go to the node's error output. The connector performs one attempt, since a failed connection can occur after a tool has already acted.
