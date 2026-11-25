# Cloudflare MCP Portals

Use a Cloudflare MCP Portal connection with the **MCP Client** workflow node. Cloudflare One manages upstream identities and policies. Store the service token in Worker secrets and let workflows select only a connection alias and an approved tool.

1. In Cloudflare One, create an [Access service token](https://developers.cloudflare.com/cloudflare-one/access-controls/service-credentials/service-tokens/) and include it in a **Service Auth** policy for the portal and each linked MCP server the workflow needs. Configure the portal to use its admin credentials for those upstream servers. Service-token sessions cannot use upstreams that require per-user OAuth. See [Cloudflare's portal setup](https://developers.cloudflare.com/cloudflare-one/access-controls/ai-controls/mcp-portals/#connect-with-a-service-token).
2. Store the Access service token's ID and secret as separate Worker secrets. These are Access service credentials rather than a Cloudflare API token.

   ```sh
   pnpm exec wrangler secret put MCP_PORTAL_CLIENT_ID
   pnpm exec wrangler secret put MCP_PORTAL_CLIENT_SECRET
   ```

3. In the existing `MCP_CONNECTIONS` Worker secret, add an alias with the portal's Streamable HTTP endpoint and a list of workspace IDs and tool names. Reference the two Worker secret names in `accessServiceToken`.

   ```json
   {
     "booking-portal": {
       "url": "https://portal.example.com/mcp",
       "workspaceIds": ["your-workspace-durable-object-id"],
       "allowedTools": ["check_availability"],
       "accessServiceToken": {
         "clientIdBinding": "MCP_PORTAL_CLIENT_ID",
         "clientSecretBinding": "MCP_PORTAL_CLIENT_SECRET"
       }
     }
   }
   ```

4. In Kasuri, open **Connections → MCP servers** to verify that the workspace can see the configured alias. **Configured** means the settings and referenced secrets are present. A live **List approved tools** or **Call a tool** run checks the actual portal policies. In the editor, pick the portal alias in MCP Client. Use the exact tool names returned by the portal.

Rotate the token in Cloudflare One and update the Worker secrets. Existing workflows keep their connection alias, so this requires no workflow edits. The connector applies the Access headers to the configured endpoint only, refuses redirects and redacts the credential values from results. Use either `accessServiceToken` or `bearerToken` on a connection. Offline simulations need only a fixture and do not contact the portal.
