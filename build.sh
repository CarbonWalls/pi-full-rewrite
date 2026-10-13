#!/usr/bin/env bash
# All cargo/rustup state stays inside this directory (no global installs).
set -e
cd "$(dirname "$0")"
export RUSTUP_HOME="$PWD/.rustup"
export CARGO_HOME="$PWD/.cargo-cache"
export PATH="$PWD/.cargo-cache/bin:$PATH"
exec cargo "$@"
