#!/usr/bin/env bash
# Install pi into a user bin directory (DEFAULT: ~/.local/bin; override with
# PREFIX). Everything outside this script's arguments stays untouched.
#
#   ./scripts/install.sh            # copies dist/pi (release) to ~/.local/bin/pi
#   PREFIX=~/bin ./scripts/install.sh
set -euo pipefail
cd "$(dirname "$0")/.."

BIN="dist/pi"
[ -x "$BIN" ] || { echo "run ./scripts/release.sh first (dist/pi missing)"; exit 1; }

PREFIX="${PREFIX:-$HOME/.local/bin}"
mkdir -p "$PREFIX"
install -m 0755 "$BIN" "$PREFIX/pi"
echo "installed pi to $PREFIX/pi"

# convenience: append a PATH line to .profile if not already present
PROFILE="${HOME}/.profile"
if ! grep -q "$PREFIX" "$PROFILE" 2>/dev/null; then
  echo "export PATH=\"$PREFIX:\$PATH\"" >> "$PROFILE"
  echo "added $PREFIX to PATH in $PROFILE (open a new shell)"
fi

"$PREFIX/pi" --info | head -1
