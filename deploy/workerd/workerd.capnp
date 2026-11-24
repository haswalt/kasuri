using Workerd = import "/workerd/workerd.capnp";
const config :Workerd.Config = (
  services = [
    (name = "internet", network = (allow = ["public"], tlsOptions = (trustBrowserCas = true))),
    (name = "main", worker = (
      modules = [
        (name = "index.js", esModule = embed "worker/index.js"),
        (name = "index_bg.wasm", wasm = embed "worker/index_bg.wasm")
      ],
      compatibilityDate = "2026-11-18",
      bindings = [
        (name = "WORKSPACES", durableObjectNamespace = "WorkflowWorkspace"),
        (name = "ASSETS", service = "assets"),
        (name = "BASIC_AUTH", fromEnvironment = "BASIC_AUTH"),
        (name = "PUBLIC_ORIGIN", fromEnvironment = "PUBLIC_ORIGIN"),
        (name = "HTTP_ALLOWED_HOSTS", fromEnvironment = "HTTP_ALLOWED_HOSTS"),
        (name = "HTTP_PRIVATE_HOSTS", fromEnvironment = "HTTP_PRIVATE_HOSTS"),
        (name = "MAX_ACTIVE_RUNS", fromEnvironment = "MAX_ACTIVE_RUNS"),
        (name = "WORKSPACE_STORAGE", fromEnvironment = "WORKSPACE_STORAGE"),
        (name = "HTTP_SECRETS", fromEnvironment = "HTTP_SECRETS")
      ],
      durableObjectNamespaces = [(className = "WorkflowWorkspace", uniqueKey = "workspaces", enableSql = true)],
      durableObjectStorage = (localDisk = "storage")
    )),
    (name = "assets", disk = (path = "public")),
    (name = "storage", disk = (path = "../data", writable = true))
  ],
  sockets = [(name = "http", address = "*:8080", http = (), service = "main")]
);
