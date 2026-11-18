# Deploy to Cloudflare

The README button targets the public source repository at `https://github.com/haswalt/kasuri`. It uses Cloudflare's official [Deploy to Cloudflare flow](https://developers.cloudflare.com/workers/platform/deploy-buttons/): the user signs in, imports the source into a repository they own, chooses deployment settings, and Cloudflare builds and deploys the Worker. The repository must be public before the button can be used by others; the button cannot deploy an unpublished local checkout.

The repository root is the deployment root. Do not point the button at `apps/web` or `apps/api`: the build needs shared contracts and Rust crates from the monorepo root. No existing Cloudflare account IDs, API tokens or deployment credentials are committed.

## Build and provision

The root `build` script checks/installs pinned Rust 1.100.0, the Wasm target and wasm-bindgen CLI 0.2.130, then compiles the engine and React UI. Rust setup uses the official rustup installer and Cargo registry. The first build can take longer while installing the binding generator; subsequent builds reuse available toolchain/cache state.

Cloudflare detects the root `build` and `deploy` scripts. The default deploy command rebuilds and type-checks before calling Wrangler. If you configure the build command separately as `pnpm build`, the deploy command can be `pnpm exec wrangler deploy` to avoid the repeated build. Keep the lockfile, package-manager version and Wrangler configuration in the repository.

`wrangler.jsonc` defines one Worker with static assets, a SQLite-backed Durable Object binding and the `v1` class migration. The button flow provisions these resources in the user's own account. No external database, R2 bucket, D1 database or queue is required by the current core-node demo.

Set `SESSION_TTL_HOURS` to `0` if you want new workspaces in your deployment to retain their data. The public demo uses `24`. This is an application setting and does not change Cloudflare's quotas or billing. Both modes still use browser-isolated workspaces; configure Cloudflare Access if your own deployment should be private to your organization.

## Publishing or forking

The button is already configured for `haswalt/kasuri`. A fork maintainer can regenerate it:

```sh
pnpm setup:repository https://github.com/YOUR-ORG/YOUR-REPO
```

This updates only the README badge. It does not create a repository, configure Git remotes, push code, create a Cloudflare account or deploy anything. Review and commit the changed README before publishing.

Once the repository is public, complete one deployment through the button to verify Cloudflare's hosted import/build path for that account. Local source builds and Wrangler deployment checks do not substitute for that first hosted import. It cannot be verified before publication.

## Manual alternative

```sh
pnpm install --frozen-lockfile
pnpm exec wrangler login
pnpm deploy:check
pnpm deploy
```

The existing hosted demo and its data remain separate from a new user's deployment. Preserve migration history and class names when upgrading an existing Worker.
