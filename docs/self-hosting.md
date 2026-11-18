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

The `/data` volume holds SQLite-backed Durable Object storage, including workflows, checkpoints, alarms and run history. Keep a stable volume and namespace key when upgrading. Browser cookies select workspaces; clearing cookies loses access to those workspaces. There are no seeded workflows or workspace expiry timers.

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

The process runs from `dist/`, serves on port 8080 and stores data in the root `data/` directory. `dist/` contains the Wasm binary, generated module, assets and `workerd.capnp`. Distribute it with a compatible workerd binary if Node.js is unavailable on the destination. From that directory run `workerd serve workerd.capnp`. The binary itself requires no Node.js runtime.

For a different address or storage path, use workerd's `--socket-addr http=127.0.0.1:8090` and `--directory-path storage=/absolute/data/path` options. Create the storage directory first.
