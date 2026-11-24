<!--
Title: use a Conventional Commit header, e.g. `fix(engine): retry a failed wait`.
CI rejects other titles. See CONTRIBUTING.md for types and scopes.
-->

## Summary

<!-- What changes and why. Link issues with "Closes #123". -->

## Changes

-

## Testing

<!-- Commands you ran and what they showed. Note anything you could not run. -->

- [ ] `pnpm test` (or the relevant subset: `pnpm test:web`, `pnpm test:rust`, `pnpm test:runtime`)
- [ ] `cargo fmt --all -- --check` and `cargo clippy --workspace --all-targets`
- [ ] New or changed behaviour has tests

## Checklist

- [ ] Docs in `docs/` or `README.md` updated for user-visible behaviour
- [ ] Workflow or API format changes stay compatible, or the break is described above
- [ ] No secrets, tokens or private hosts in code, fixtures or examples
