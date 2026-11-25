# Third-party platform authentication

Status: required feature; provider integration is not implemented yet. This is the implementation contract, not a claim that configuring an identity provider currently enables application-level identity.

Kasuri must delegate user authentication to a third party. It will not implement password accounts, password recovery, MFA or its own identity service. Cloudflare hosting must not mandate Cloudflare Access: the same provider interface must support other deployments and other providers on Cloudflare.

## Current behavior

The current runtime supports installation-wide Basic authentication through `BASIC_AUTH`, while browser cookies identify an owner directory and its workspaces. That cookie is not a third-party identity, shared membership system or account recovery mechanism. MCP has separate scoped bearer credentials/OAuth grants; webhook triggers have their own selected authentication. Existing Basic authentication is compatibility infrastructure, not the intended production identity model.

An external identity proxy can protect access today, but does not automatically map its users to Kasuri accounts. Keep the runtime private behind the proxy and retain existing protection until the new integration has been implemented and verified. Protect dashboard API routes as well as HTML; protecting only the login page is insufficient.

## Provider boundary

Implement a Rust authentication boundary which converts a verified third-party identity to a normalized principal: provider ID, issuer and stable subject, with optional verified display/email/group claims. Identity keys use issuer + subject, never mutable email alone. Workspace membership, authorization and audit attribution remain Kasuri responsibilities.

Initial adapter paths:

- Cloudflare Access: verify the signed assertion against configured issuer, application audience and trusted signing keys. Reject unsigned, expired, wrong-audience and wrong-issuer assertions. Header presence alone is not authentication. Cover custom domains and any alternative origin routes so users cannot bypass Access.
- Standards-based OIDC: integrate a maintained authentication library/provider SDK compatible with Rust/Wasm, using provider discovery, authorization code + PKCE, state and nonce. Delegate sign-in and MFA to the configured provider. Do not write an OAuth/OIDC protocol implementation ourselves.
- Trusted authentication proxy: support third-party identity middleware without making arbitrary client headers authoritative. Require a verified signed assertion, or a tightly enforced private/mutually authenticated proxy boundary with incoming identity headers stripped and replaced. Specify and test that trust contract before enabling an adapter.

Cloudflare Access is an optional adapter on Cloudflare. Provider choice is independent of where workflow execution runs. Secrets and provider registration live in installation configuration, never workflow exports or public examples. Proposed configuration names are not usable runtime settings until implementation lands.

## Protected surfaces and separate credentials

Require authenticated principals for the dashboard, management `/api/*` endpoints and browser workspace mutations. Return a structured 401/403 to API clients rather than HTML login pages. Keep same-origin/CSRF protection for browser mutations. Any application session delegates authentication to the provider; use secure, HttpOnly, appropriately SameSite cookies with defined expiry/logout behavior.

MCP transport continues to use its audience-bound token/OAuth flows and explicit scopes. Browser consent must require the authenticated owner and workspace permissions. Do not turn an MCP bearer token into a dashboard login. Webhook paths retain their published trigger auth policy; do not put an interactive dashboard login in front of public triggers. Health probes reveal no application data. Document ingress exceptions for `/hooks/*`, MCP, OAuth/discovery and health according to the selected deployment topology.

## Ownership migration and acceptance

Migrate existing cookie-owned directories through an explicit, authenticated owner-claim procedure with proof of existing ownership. Never silently assign existing workspaces to the first user who logs in. Retain compatibility until owners can migrate; keep migration atomic, audited and reversible from a backup. Define workspace owner/member roles and default-deny access before enabling multiple users.

Test the provider contract on Cloudflare: valid sign-in, expired/revoked sessions and key rotation, forged identity headers, origin bypass, two-user workspace isolation, CSRF, logout, owner migration and unchanged MCP/webhook authentication. Missing provider configuration must fail closed in production mode, with a separate explicit loopback development mode. Deployment helpers must identify a third-party auth prerequisite rather than create local password accounts as the final production setup.

Reference: [Cloudflare Access JWT validation](https://developers.cloudflare.com/cloudflare-one/access-controls/applications/http-apps/authorization-cookie/validating-json/). Access assertions require cryptographic validation, including audience and issuer checks; adapter implementation must verify current provider documentation and select a maintained compatible library.
