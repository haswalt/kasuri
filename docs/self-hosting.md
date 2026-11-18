# Self-hosting

The same Rust Wasm Worker runs on the open-source workerd runtime in Docker or Kubernetes. The container contains workerd, the compiled Worker and React assets. Node.js is used for build/test tooling only and is absent from the runtime image.

## Docker Compose

```sh
node scripts/setup-selfhost.mjs
# Read secrets/basic_auth locally to obtain the generated username:password.
docker compose up --build -d
```

Open `http://localhost:8080` and sign in. The setup helper keeps existing credentials. If Node.js is unavailable, create `secrets/basic_auth` yourself containing `username:password`, with permissions allowing container UID 1000 to read it. Never commit this file.

Compose binds to loopback by default. Put a TLS reverse proxy in front when exposing the service. Change the host port with `PORT=8090 docker compose up -d`.

```sh
docker build -t kasuri:local .
node scripts/test-container.mjs kasuri:local
```

Container checks create their own temporary workflows, verify Basic authentication, simulations and wait recovery after restarting, and clean up their container, secret directory and volume.

The image runs as UID/GID 1000 with a read-only filesystem, writable `/data`, a temporary `/tmp`, no capabilities and no privilege escalation. `BASIC_AUTH_FILE` supplies a mounted credential file; `BASIC_AUTH` can also supply credentials directly. `/api/health` is available without authentication for probes.

## Persistence and upgrades

The `/data` volume holds SQLite-backed Durable Object storage, including workflows, checkpoints, alarms, run history, MCP credential hashes, OAuth grants, mutation receipts and retained audit records. Keep a stable volume and namespace key when upgrading. Browser cookies select workspaces; clearing cookies loses access to those workspaces. There are no seeded workflows or workspace expiry timers.

Run one replica per storage volume. Do not mount the same SQLite storage into multiple workerd processes. Stop the service before taking a filesystem backup, then restart it. Restore the complete volume into a stopped installation using the same namespace configuration. Do not use `docker compose down -v` unless you intend to delete data.

This runtime replaces the former Node server and does not migrate its database format. Start with a fresh data volume, or export/import workflow JSON before switching.

## Kubernetes

Build and publish the image to your own registry, then set that image in `deploy/k8s/deployment.yaml`. Review the PVC storage class and requested size before applying.

```sh
kubectl create namespace kasuri
kubectl -n kasuri create secret generic kasuri-auth --from-file=credentials=secrets/basic_auth
kubectl -n kasuri apply -k deploy/k8s
kubectl -n kasuri port-forward service/kasuri 8080:80
```

The manifests use one replica with a Recreate strategy, a persistent data volume, a read-only secret mount and HTTP probes. Add your own TLS ingress, domain and network policies. Do not increase replicas while using the local SQLite volume.

## Run without Docker

Use Node.js 24+, pnpm and Rust for the build. workerd is installed as a pinned development dependency.

```sh
pnpm install
pnpm build:selfhost
BASIC_AUTH='username:password' pnpm start
```

The process runs from `dist/`, serves on port 8080 and stores data in the root `data/` directory. `dist/` contains the Wasm binary, generated module, assets and `workerd.capnp`. Distribute it with a compatible workerd binary if Node.js is unavailable on the destination. From that directory run `PUBLIC_ORIGIN= BASIC_AUTH= HTTP_ALLOWED_HOSTS= HTTP_PRIVATE_HOSTS= HTTP_SECRETS= workerd serve workerd.capnp`. The provided launchers supply an empty default for the optional workerd environment binding. The binary itself requires no Node.js runtime.

For a different address or storage path, use workerd's `--socket-addr http=127.0.0.1:8090` and `--directory-path storage=/absolute/data/path` options. Create the storage directory first.

## Outbound HTTP connectors

Public HTTPS works by default. Optionally set `HTTP_ALLOWED_HOSTS` to restrict destinations. Basic authentication and sensitive custom headers resolve aliases from `HTTP_SECRETS` (a JSON map); use an environment secret, Kubernetes Secret or an `HTTP_SECRETS_FILE` mount. `HTTP_PRIVATE_HOSTS` opts specific private hosts into the application policy, but workerd network permissions/connectivity must also allow those destinations. Redirects remain blocked. See [connector configuration](connectors.md).

## MCP and external HTTPS origins

Every workspace exposes a Rust MCP endpoint; see [MCP setup](mcp.md). When an ingress or reverse proxy terminates TLS before workerd, set `PUBLIC_ORIGIN` to the canonical external HTTPS origin, without a trailing slash or path. Docker Compose passes this variable through; on Kubernetes, add it to the Deployment's environment. The API then uses that configured origin for consent redirects, MCP discovery, same-origin checks and Secure cookies. Forward `/mcp/*`, `/oauth/*` and `/.well-known/*` to workerd as well as `/api/*` and the UI. Do not rely on client-supplied forwarded headers to select an origin.

For direct loopback development, leave `PUBLIC_ORIGIN` empty. The provided shell/container launchers supply this default, including when running a packaged release outside Docker. Keep the external origin stable: credentials are audience-bound to its workspace endpoint, so changing it requires new credentials or OAuth authorization. Back up the complete volume to preserve grant revocations and idempotency receipts along with workflow data.
