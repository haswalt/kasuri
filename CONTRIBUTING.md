# Contributing to Kasuri

Thanks for helping improve Kasuri. This guide covers local setup, the checks CI runs, and how commits and pull requests are named.

## Getting set up

You need:

- **Node.js 24+**
- **pnpm**, at the version pinned in `package.json` (`packageManager`)
- **Rust** via [rustup](https://rustup.rs). `rust-toolchain.toml` pins the compiler and the `wasm32-unknown-unknown` target, and the build helper installs the pinned `worker-build` CLI.

```sh
pnpm install
pnpm dev
```

`pnpm dev` builds the Rust Worker and the web app, then starts Wrangler locally. For frontend-only work, run `pnpm dev:web` alongside it. See the [README](README.md) for architecture, and `docs/` for connectors, webhooks, MCP and self-hosting.

## Branches

- Branch from `develop` and open pull requests against `develop`.
- Keep each pull request to one logical change. Split unrelated fixes into separate PRs. When one change builds on another, stack PRs and say so in the description.

## Commit messages and pull request titles

Kasuri uses [Conventional Commits](https://www.conventionalcommits.org/) for its changelog. Pull requests are **squash-merged**, and the PR title becomes the commit subject on `develop`, so CI checks every PR title.

```
<type>(<optional scope>): <summary>
```

- **Types:** `feat`, `fix`, `perf`, `refactor`, `docs`, `test`, `build`, `ci`, `chore`, `style`, `revert`
- **Scope:** an optional lowercase area, such as `engine`, `mcp`, `webhooks`, `data`, `editor`, `service`, `schema` or `docker`
- **Summary:** imperative and lowercase, with no trailing period. The whole header can be up to about 100 characters.
- **Breaking changes:** add `!` after the type or scope (for example `feat(api)!: require edit versions`) and explain the break in the PR description.

Examples:

```
feat(webhooks): expose immutable credential-safe request context
fix(engine): drop the expression context once input bindings resolve
docs(mcp): describe input bindings and the workflow op in discovery
```

Individual commits on a branch can be informal, since squash merging replaces them. Conventional commit messages still make review easier.

## Checks

CI runs these on every pull request. Run the relevant ones before pushing:

| Check | Command |
|---|---|
| Rust formatting | `cargo fmt --all -- --check` |
| Rust lints (correctness and suspicious are enforced) | `cargo clippy --workspace --all-targets` |
| Web lint | `pnpm --filter web lint` |
| Build | `pnpm build:selfhost` |
| Web tests | `pnpm test:web` |
| Rust tests | `pnpm test:rust` (`cargo test --workspace`) |
| Runtime tests against a local Worker | `pnpm test:runtime` |
| Container image | `docker build -t kasuri:ci . && node scripts/test-container.mjs kasuri:ci` |

`pnpm test` runs the web, Rust and runtime suites together. For a focused Rust change, `cargo test -p <crate>` is a quick first check.

## Writing changes

- **Tests:** add or update tests with every behaviour change. Rust crates keep integration tests in `crates/<crate>/tests`, the web app in `apps/web/tests`, and end-to-end runtime checks in `scripts/*-checks.mjs`.
- **Examples:** `examples/workflows` are compiled into the MCP discovery tools and asserted in CI. Keep them valid and free of real credentials.
- **Compatibility:** saved workflows and the HTTP and MCP APIs are persistent contracts. Don't break existing documents silently. Changes to the workflow format go through `schemaVersion`.
- **Secrets:** never commit tokens, webhook keys or private hostnames. Use `.env.example` for configuration names only.
- **Docs:** update `README.md` or the relevant page in `docs/` when user-visible behaviour changes.

## Reporting issues

Use the issue forms for bugs and feature requests. Report security vulnerabilities privately through [GitHub security advisories](https://github.com/haswalt/kasuri/security/advisories/new), not in a public issue.
