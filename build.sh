#!/usr/bin/env bash
# All cargo/rustup state stays inside this directory (no global installs).
# When no in-tree toolchain exists (e.g. a fresh CI checkout), the system
# cargo/rustup provided by the environment is used instead.
set -e
cd "$(dirname "$0")"
if [ -x "$PWD/.cargo-cache/bin/cargo" ]; then
  export RUSTUP_HOME="$PWD/.rustup"
  export CARGO_HOME="$PWD/.cargo-cache"
  export PATH="$PWD/.cargo-cache/bin:$PATH"
  exec "$PWD/.cargo-cache/bin/cargo" "$@"
fi
exec cargo "$@"
