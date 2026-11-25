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

The process runs from `dist/`, serves on port 8080 and stores data in the root `data/` directory. `dist/` contains the Wasm binary, generated module, assets and `workerd.capnp`. Distribute it with a compatible workerd binary if Node.js is unavailable on the destination. From that directory run `PUBLIC_ORIGIN= BASIC_AUTH= HTTP_ALLOWED_HOSTS= HTTP_PRIVATE_HOSTS= HTTP_SECRETS= MAX_ACTIVE_RUNS= RUN_HISTORY= RUN_HISTORY_DAYS= FAILED_RUN_HISTORY_DAYS= RUN_EXECUTION= workerd serve workerd.capnp`. The provided launchers supply an empty default for the optional workerd environment binding. The binary itself requires no Node.js runtime.

Unset/empty `RUN_EXECUTION` now uses run objects; set `workspace` explicitly for the legacy execution fallback. All modes still share one local workerd process and are not a multi-process cluster.

`RUN_EXECUTION=queues` needs a message queue, which workerd provides with its built-in Queues Worker, storing messages on the same data volume. workerd marks that Worker experimental, so it is not in `workerd.capnp`: the build also writes `workerd-queues.capnp`, and the launchers (`pnpm start` and the container) use it, with `--experimental`, only when `RUN_EXECUTION=queues`. Run it directly with `workerd serve workerd-queues.capnp --experimental`. The other modes don't use it.

For a different address or storage path, use workerd's `--socket-addr http=127.0.0.1:8090` and `--directory-path storage=/absolute/data/path` options. Create the storage directory first.

## Outbound HTTP connectors

Public HTTPS works by default. Optionally set `HTTP_ALLOWED_HOSTS` to restrict destinations. Basic authentication and sensitive custom headers resolve aliases from `HTTP_SECRETS` (a JSON map); use an environment secret, Kubernetes Secret or an `HTTP_SECRETS_FILE` mount. `HTTP_PRIVATE_HOSTS` opts specific private hosts into the application policy, but workerd network permissions/connectivity must also allow those destinations. Redirects remain blocked. See [connector configuration](connectors.md).

## MCP and external HTTPS origins

Every workspace exposes a Rust MCP endpoint; see [MCP setup](mcp.md). When an ingress or reverse proxy terminates TLS before workerd, set `PUBLIC_ORIGIN` to the canonical external HTTPS origin, without a trailing slash or path. Docker Compose passes this variable through; on Kubernetes, add it to the Deployment's environment. The API then uses that configured origin for consent redirects, MCP discovery, same-origin checks and Secure cookies. Forward `/mcp/*`, `/oauth/*` and `/.well-known/*` to workerd as well as `/api/*` and the UI. Do not rely on client-supplied forwarded headers to select an origin.

For direct loopback development, leave `PUBLIC_ORIGIN` empty. The provided shell/container launchers supply this default, including when running a packaged release outside Docker. Keep the external origin stable: credentials are audience-bound to its workspace endpoint, so changing it requires new credentials or OAuth authorization. Back up the complete volume to preserve grant revocations and idempotency receipts along with workflow data.

## Scaling and availability

The shipped package is a **single stateful workerd process**, not a distributed worker pool. All workspaces share that process. On Cloudflare, by contrast, each workspace is its own Durable Object, so workspaces spread across machines. On both platforms a single workspace is one Durable Object, so its runs share one thread and one storage budget; concurrent runs are bounded by that budget, or by `MAX_ACTIVE_RUNS` if set. `deploy/workerd/workerd.capnp` binds Durable Object persistence to local disk; Kubernetes uses one replica and a Recreate rollout. HTTP connector calls and explicit workflow branches can overlap inside that process, subject to existing workspace/run concurrency budgets. This is execution concurrency, not horizontal worker scaling.

| Deployment change | Supported today |
| --- | --- |
| Increase CPU/memory and persistent disk for the existing instance | Yes; measure workload throughput and runtime budgets. |
| Restart/reschedule the single pod with the complete persistent volume | Yes; recover persisted runs/alarms. This entails downtime, not seamless failover. |
| Add replicas sharing the same SQLite volume | No; the package has no cross-process ownership or locking protocol for this topology. |
| Add replicas with separate volumes behind round-robin ingress | No for one installation; each volume becomes independent state and schedules. Sticky HTTP sessions do not coordinate alarms, OAuth grants, runs or receipts. |
| Run independent installations with their own domains, data and credentials | Yes; this partitions installations, not one shared workspace. |

Do not enable an HPA on the supplied stateful Deployment. A ReadWriteOnce PVC is not a distributed execution lock. Backups, restore and replica restart instructions above apply to the whole installation, including credential grants, run checkpoints, alarms and mutation receipts.

### Future multi-worker deployment

A proposed first step, placing workspaces on several independent single-node shards behind a stateless router, is described in the [workspace sharding design](self-host-sharding.md). It is not implemented; the guidance on this page still applies.

Horizontal execution is a separate implementation feature. Before supporting replicas, choose and implement a tested coordination/storage backend: workspace/object ownership with fencing, durable work claiming or queues, transactional checkpoint/effect receipts, alarm ownership and due-work recovery, cancellation/pause propagation, shared authentication/authorization state and consistent artifact access. Handle abandoned leases and duplicate delivery without repeating completed effects. Preserve the Rust execution core and deployable Wasm backend; a TypeScript bridge is not required.

Evaluate runtime-managed Durable Object clustering against the pinned workerd version and its recovery guarantees, or implement a compatible distributed host/storage adapter. The upstream runtime's [configuration schema](https://github.com/cloudflare/workerd/blob/main/src/workerd/server/workerd.capnp) evolves; upstream clustering options do not imply Kasuri's current package configures or supports them. Sharing SQLite or merely adding a queue is insufficient.

Acceptance must include multiple real processes, worker loss during dispatch/completion, no split-brain owners, receipt replay, restart-safe schedules, fair bounded work distribution, draining upgrades and documented recovery/backup procedures. Until those checks exist, the supported self-host topology remains one active process per installation volume.

## Third-party dashboard authentication

Third-party authentication is a required upcoming feature, with Cloudflare Access, OIDC and trusted identity-proxy adapters independent of hosting platform. See [the provider and ownership migration contract](platform-auth.md). Current Basic authentication and cookie ownership do not yet provide that identity model. Dashboard/API protection, agent MCP authentication and workflow webhook authentication are separate boundaries.
