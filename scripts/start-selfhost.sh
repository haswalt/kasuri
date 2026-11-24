#!/bin/sh
# Launch the Rust/Wasm runtime; no Node application server or proxy is involved.
set -eu
# workerd's environment bindings must have explicit defaults when optional.
export PUBLIC_ORIGIN="${PUBLIC_ORIGIN:-}"
export BASIC_AUTH="${BASIC_AUTH:-}"
export HTTP_ALLOWED_HOSTS="${HTTP_ALLOWED_HOSTS:-}"
export HTTP_PRIVATE_HOSTS="${HTTP_PRIVATE_HOSTS:-}"
export MAX_ACTIVE_RUNS="${MAX_ACTIVE_RUNS:-}"
export WORKSPACE_STORAGE="${WORKSPACE_STORAGE:-}"
export HTTP_SECRETS="${HTTP_SECRETS:-}"
cd "$(dirname "$0")/../dist"
exec ../node_modules/workerd/bin/workerd serve workerd.capnp "$@"
