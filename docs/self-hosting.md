# Self-hosting Kasuri

Kasuri can run on Cloudflare Workers or on a server/container you control. Both hosts use the same Rust/Wasm engine, API, workflow model, node implementations and frontend. The standalone host uses local SQLite databases and persisted timers instead of Cloudflare bindings. It does not require a Cloudflare account.

These instructions cover the currently implemented core nodes. HTTP, SaaS and native Cloudflare resource connectors are still planned; packaging does not turn them into working adapters. A future Cloudflare-specific connector will require its own credentials/remote adapter when used outside Cloudflare.

## Docker Compose

Requirements: Docker Engine with Compose. Node.js is needed only for the optional credential-generation helper; Rust and build dependencies are installed inside the multi-stage Docker build.

```sh
git clone https://github.com/haswalt/kasuri.git
cd kasuri
node scripts/setup-selfhost.mjs
docker compose up --build -d
```

Open `http://localhost:8080`. Sign in with the `user:password` pair stored in `secrets/basic_auth` (the generated username is `admin`). The helper preserves an existing credential file. The containing directory is mode 0700; the file is readable inside the non-root container. The secret directory and local configuration files are ignored by Git and excluded from Docker's build context. Do not publish them.

Without Node.js on the host, create the file yourself before starting Compose:

```sh
mkdir -m 700 secrets
# Write a user:password pair to secrets/basic_auth with your preferred secret tool.
# The password must contain at least 12 characters. Use file mode 0644 inside this private directory.
```

Compose builds a runtime image containing Node.js 24, the compiled Wasm engine and the built React UI. The runtime uses UID 1000, a read-only root filesystem, a writable `/data` volume, a small `/tmp` tmpfs, no Linux capabilities and no privilege escalation. A health check probes `/api/health`. Authentication protects the UI and other API endpoints.

The default port is bound to the loopback interface. Put a TLS reverse proxy in front of it for remote access. Copy `.env.example` to `.env` and set `PUBLIC_ORIGIN` to the exact external origin, such as `https://kasuri.example.com`; this enables Secure session cookies and correct same-origin checks. If you change `PORT`, update the port in `PUBLIC_ORIGIN` too. Configure your proxy to preserve the request path and forward to port 8080.

```sh
docker compose logs -f kasuri
docker compose stop
# Rebuild after pulling source updates:
docker compose up --build -d
```

The named `kasuri-data` volume stores workflows, run snapshots, persisted wake-ups and cron registration. Recreating the container retains this data. Do not use `docker compose down --volumes` unless you intend to remove it. To back up, stop the service, snapshot/copy the entire data volume, then start it again. Restore the volume with ownership writable by UID/GID 1000.

## Build an image directly

```sh
docker build -t kasuri:local .
```

The build compiles Rust for `wasm32-unknown-unknown` and generates matching JavaScript bindings; it does not copy generated binaries from your checkout. It then builds the frontend and standalone server. The final image does not need pnpm, Cargo or network access to Cloudflare.

To verify an image, including persistent wait recovery after restart:

```sh
node scripts/test-container.mjs kasuri:local
```

The test creates its own disposable container and volume and removes both afterward. It does not inspect or delete an existing deployment's data. For multiple CPU architectures, use Docker Buildx and a registry you control; only the locally built architecture is verified by this repository's default test command.

## Kubernetes

`deploy/k8s` includes a Namespace, single-replica Deployment, ClusterIP Service and 2 GiB ReadWriteOnce PVC. The pod uses startup/readiness/liveness probes, resource requests/limits, non-root execution, a read-only root filesystem and a mounted authentication secret.

1. Build and push your image to a registry your cluster can pull from. No prebuilt public image is assumed.
2. Update `images.newName` and `images.newTag` in `deploy/k8s/kustomization.yaml`. For a local cluster, load `kasuri:local` into that cluster's image store instead.
3. Generate the credential file as above, then create the namespace and secret:

```sh
kubectl apply -f deploy/k8s/namespace.yaml
kubectl -n kasuri create secret generic kasuri-auth --from-file=credentials=secrets/basic_auth
kubectl apply -k deploy/k8s
kubectl -n kasuri rollout status deployment/kasuri
kubectl -n kasuri port-forward service/kasuri 8080:80
```

A default StorageClass must be available, or set `storageClassName` on the PVC. The filesystem must support SQLite's file locking. If you add an Ingress, configure TLS, keep the application behind the supplied authentication or your own identity proxy, and set the external origin:

```sh
kubectl -n kasuri set env deployment/kasuri PUBLIC_ORIGIN=https://kasuri.example.com
```

Use one replica with the supplied `Recreate` deployment strategy. This SQLite host is not an HA/multi-writer deployment. A persistent SQLite ownership lock rejects a second server sharing the same data directory, and the OS releases that lock after a crash. Do not switch to rolling updates or increase replicas without replacing the storage/scheduling coordination layer.

The manifests have been rendered locally with `kubectl kustomize`; they are not automatically applied to your cluster. Cluster ingress, registry credentials, storage provisioning and backups remain under the operator's control.

## Run without containers

Use Node.js 24+, pnpm and a Rust toolchain. The build helper can install the pinned Rust toolchain, Wasm target and wasm-bindgen CLI from their official sources. To install/check those separately, run `node scripts/ensure-toolchain.mjs --install`.

```sh
pnpm install --frozen-lockfile
pnpm build:selfhost
node scripts/setup-selfhost.mjs
BASIC_AUTH_FILE=secrets/basic_auth pnpm start
```

`pnpm start` serves the bundled server and copied frontend from `dist/`. Set `DATA_DIR` to a persistent writable directory. SIGTERM/SIGINT stop accepting requests and close the databases cleanly; pending waits and cron schedules are reconstructed when the process starts again. A single owner must run against each data directory.

## Configuration

| Variable | Default | Purpose |
| --- | --- | --- |
| `PORT` | `8080` | HTTP listening port |
| `HOST` | `0.0.0.0` | Bind interface |
| `DATA_DIR` | `./data` for source startup; `/data` in Docker | Persistent SQLite files |
| `ASSETS_DIR` | Bundled `dist/public`; `/app/public` in Docker | Built frontend assets |
| `PUBLIC_ORIGIN` | Incoming HTTP origin | External HTTPS origin when running behind a proxy |
| `BASIC_AUTH_FILE` | Unset for raw server; required by Compose/Kubernetes examples | File containing one `user:password` pair |
| `SESSION_TTL_HOURS` | `0` standalone; `24` public Cloudflare demo | `0` retains workspace data; 1–8760 expires new workspaces after that many hours |

Basic authentication is a deployment access gate, not workspace membership or RBAC. Browser workspaces still use isolated opaque session cookies. Persistent mode retains server data but does not recover a lost browser cookie; export workflows and keep backups. Session lifetime changes apply to newly created workspaces, not existing expiry timestamps. The finite browser cookie lifetime is one year in persistent mode.

The same graph, payload and run resource budgets apply on both hosts. Self-hosting does not yet add account authentication, workspace sharing, a credential vault, external side-effect journaling, distributed scheduling or production connector parity.
