# Platform sign-in and workspace ownership

Cloudflare Access is the first identity provider supported by Kasuri. It handles sign-in. The Kasuri Worker verifies the signed application assertion and uses the issuer and subject to open that user's workspace directory. Different browser profiles for the same user see the same workspaces. Kasuri uses email for display.

A deployment without a configured identity provider denies dashboard and browser API access. The explicitly enabled local mode used by `pnpm dev` uses browser sessions on loopback.

## Configure Cloudflare Access

1. Create a self-hosted Access application for the Kasuri hostname and authorize the intended users through your identity provider. Use a hostname that can route dashboard and machine endpoints separately. Cover the dashboard, browser API, and static assets.
2. Keep MCP transport, OAuth machine endpoints and discovery, published webhook paths, and the health probe reachable under their own authentication rules. For a single hostname, configure more specific Access bypass applications as needed for `/mcp/*`, `/oauth/*`, `/.well-known/*`, `/hooks/*`, `/api/hooks/*`, and `/api/health`. The browser OAuth consent page under `/w/*` and approval endpoints under `/api/mcp/*` require sign-in.
3. Set `PLATFORM_ACCESS_ISSUER` to the canonical HTTPS Access team domain, such as `https://your-team.cloudflareaccess.com`, and `PLATFORM_ACCESS_AUDIENCE` to the application's AUD tag. Set `PUBLIC_ORIGIN` to the canonical HTTPS hostname if multiple aliases are in use. Keep the original MCP hostname and endpoint working if older credentials are still in use, because credentials are bound to their original audience.
4. Deploy the Worker and sign in. Visit MCP server to see the browser's workspace ID. Run `workspace_identity_get` from an MCP client to see its workspace ID, credential and scopes. New grants also report their authorizing identity. Earlier credentials return `authorizedBy: null`.

The Worker rejects missing or invalid JWTs, wrong issuers and audiences, expired assertions, and service-token identities. It verifies signatures with the team signing keys using Workers Web Crypto. The Access edge policy and the Worker must both be configured. [Cloudflare Access JWT validation](https://developers.cloudflare.com/cloudflare-one/access-controls/applications/http-apps/authorization-cookie/validating-json/).

## Recover a legacy workspace directory

Before upgrading, identify the existing personal workspace ID you want the account to retain. Use the workspace ID from its MCP endpoint or `workspace_identity_get`, and verify its ownership from existing deployment or session records. A bearer credential alone does not grant dashboard ownership. Obtain the assigned user's Access `sub` claim from an authenticated session or the Access identity record.

The deployment operator can set the Worker secret `PLATFORM_ACCESS_LEGACY_OWNERS` to a JSON object that maps subjects to the verified personal workspace IDs. For example:

```json
{
  "access-user-subject": "existing-personal-workspace-durable-object-id"
}
```

Set the mapping before the user's first sign-in so their existing directory becomes their personal workspace. Its named workspaces, runs, webhooks and MCP endpoint IDs stay in place. Use one existing personal directory per subject and keep the mapping in deployment configuration. Changing it switches directories without merging either directory. A directory already claimed by another identity rejects the new claim. The deployment operator should review the mapping and preserve a backup of the prior configuration.
