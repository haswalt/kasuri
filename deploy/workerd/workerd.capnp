using Workerd = import "/workerd/workerd.capnp";
const config :Workerd.Config = (
  services = [
    (name = "main", worker = (
      modules = [
        (name = "index.js", esModule = embed "worker/index.js"),
        (name = "index_bg.wasm", wasm = embed "worker/index_bg.wasm")
      ],
      compatibilityDate = "2026-11-18",
      bindings = [
        (name = "WORKSPACES", durableObjectNamespace = "WorkflowWorkspace"),
        (name = "ASSETS", service = "assets"),
        (name = "BASIC_AUTH", fromEnvironment = "BASIC_AUTH")
      ],
      durableObjectNamespaces = [(className = "WorkflowWorkspace", uniqueKey = "workspaces", enableSql = true)],
      durableObjectStorage = (localDisk = "storage")
    )),
    (name = "assets", disk = (path = "public")),
    (name = "storage", disk = (path = "../data", writable = true))
  ],
  sockets = [(name = "http", address = "*:8080", http = (), service = "main")]
);
