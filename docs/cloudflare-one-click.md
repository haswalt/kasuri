# One-click Cloudflare deployment

[![Deploy to Cloudflare](https://deploy.workers.cloudflare.com/button)](https://deploy.workers.cloudflare.com/?url=https://github.com/haswalt/kasuri)

The button imports the public repository into your own account. Use the repository root, build command `pnpm build`, and deploy command `pnpm exec wrangler deploy`. The build installs the pinned Rust toolchain, Wasm target and official `worker-build` CLI, then builds the Rust Worker and React assets.

Wrangler provisions `WORKSPACES` using the `WorkflowWorkspace` SQLite Durable Object migration. No D1 database, R2 bucket or external service credentials are required for the current core connectors. New installations start empty; there is no hosted demo or expiry mode.

See [deployment details](deployment.md) for authentication and API behavior. Keep account tokens, domains and secrets in your own deployment environment. Preserve Durable Object migrations and class names on upgrades.

Forks can update the deploy button with:

```sh
pnpm setup:repository -- https://github.com/YOUR-ORG/YOUR-REPO
```

A deployment contains a generated JavaScript Wasm entrypoint from Cloudflare's Rust tooling. There is no handwritten TypeScript backend.
