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
        (name = "RUNS", durableObjectNamespace = "WorkflowRun"),
        (name = "ASSETS", service = "assets"),
        (name = "PLATFORM_AUTH", fromEnvironment = "PLATFORM_AUTH"),
        (name = "PLATFORM_ACCESS_ISSUER", fromEnvironment = "PLATFORM_ACCESS_ISSUER"),
        (name = "PLATFORM_ACCESS_AUDIENCE", fromEnvironment = "PLATFORM_ACCESS_AUDIENCE"),
        (name = "PLATFORM_ACCESS_LEGACY_OWNERS", fromEnvironment = "PLATFORM_ACCESS_LEGACY_OWNERS"),
        (name = "BASIC_AUTH", fromEnvironment = "BASIC_AUTH"),
        (name = "PUBLIC_ORIGIN", fromEnvironment = "PUBLIC_ORIGIN"),
        (name = "HTTP_ALLOWED_HOSTS", fromEnvironment = "HTTP_ALLOWED_HOSTS"),
        (name = "HTTP_PRIVATE_HOSTS", fromEnvironment = "HTTP_PRIVATE_HOSTS"),
        (name = "MAX_ACTIVE_RUNS", fromEnvironment = "MAX_ACTIVE_RUNS"),
        (name = "RUN_HISTORY", fromEnvironment = "RUN_HISTORY"),
        (name = "RUN_HISTORY_DAYS", fromEnvironment = "RUN_HISTORY_DAYS"),
        (name = "FAILED_RUN_HISTORY_DAYS", fromEnvironment = "FAILED_RUN_HISTORY_DAYS"),
        (name = "RUN_EXECUTION", fromEnvironment = "RUN_EXECUTION"),
        (name = "WORKSPACE_SECRETS_KEY", fromEnvironment = "WORKSPACE_SECRETS_KEY"),
        (name = "HTTP_SECRETS", fromEnvironment = "HTTP_SECRETS")
      ],
      durableObjectNamespaces = [(className = "WorkflowWorkspace", uniqueKey = "workspaces", enableSql = true), (className = "WorkflowRun", uniqueKey = "runs", enableSql = true)],
      durableObjectStorage = (localDisk = "storage")
    )),
    (name = "assets", disk = (path = "public")),
    (name = "storage", disk = (path = ".", writable = true))
  ],
  sockets = [(name = "http", address = "127.0.0.1:8080", http = (), service = "main")]
);
