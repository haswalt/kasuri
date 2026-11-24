#!/bin/sh
# Launch the Rust/Wasm runtime; no Node application server or proxy is involved.
set -eu
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
cd "$(dirname "$0")/../dist"
exec ../node_modules/workerd/bin/workerd serve workerd.capnp "$@"
