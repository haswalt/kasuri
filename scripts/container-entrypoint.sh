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
exec /usr/local/bin/workerd serve /app/workerd.capnp "$@"
