#!/bin/sh
set -eu
if [ -n "${BASIC_AUTH_FILE:-}" ]; then
  BASIC_AUTH=$(cat "$BASIC_AUTH_FILE")
  export BASIC_AUTH
fi
if [ -n "${HTTP_SECRETS_FILE:-}" ]; then
  HTTP_SECRETS=$(cat "$HTTP_SECRETS_FILE")
  export HTTP_SECRETS
fi
# workerd's environment bindings must have explicit defaults when optional.
export PUBLIC_ORIGIN="${PUBLIC_ORIGIN:-}"
export BASIC_AUTH="${BASIC_AUTH:-}"
export HTTP_ALLOWED_HOSTS="${HTTP_ALLOWED_HOSTS:-}"
export HTTP_PRIVATE_HOSTS="${HTTP_PRIVATE_HOSTS:-}"
export MAX_ACTIVE_RUNS="${MAX_ACTIVE_RUNS:-}"
export RUN_HISTORY="${RUN_HISTORY:-}"
export RUN_HISTORY_DAYS="${RUN_HISTORY_DAYS:-}"
export FAILED_RUN_HISTORY_DAYS="${FAILED_RUN_HISTORY_DAYS:-}"
export RUN_EXECUTION="${RUN_EXECUTION:-}"
export HTTP_SECRETS="${HTTP_SECRETS:-}"
# RUN_EXECUTION=queues uses workerd's built-in Queues Worker, which workerd
# marks experimental; the default config doesn't include it.
if [ "$RUN_EXECUTION" = "queues" ]; then
  exec /usr/local/bin/workerd serve /app/workerd-queues.capnp --experimental "$@"
fi
exec /usr/local/bin/workerd serve /app/workerd.capnp "$@"
