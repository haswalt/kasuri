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
export HTTP_SECRETS="${HTTP_SECRETS:-}"
exec /usr/local/bin/workerd serve /app/workerd.capnp "$@"
