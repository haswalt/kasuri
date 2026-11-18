#!/bin/sh
set -eu
if [ -n "${BASIC_AUTH_FILE:-}" ]; then
  BASIC_AUTH=$(cat "$BASIC_AUTH_FILE")
  export BASIC_AUTH
fi
exec /usr/local/bin/workerd serve /app/workerd.capnp "$@"
