#!/usr/bin/env bash
# Remote one-liner installer for the published release.
#
#   curl -fsSL https://github.com/CarbonWalls/pi-full-rewrite/raw/master/scripts/install-remote.sh | bash
#   PREFIX=~/bin bash <(curl -fsSL ...)
#
# Downloads the static musl binary from the latest GitHub release.
set -euo pipefail

REPO="CarbonWalls/pi-full-rewrite"
PREFIX="${PREFIX:-$HOME/.local/bin}"

case "$(uname -s)-$(uname -m)" in
  Linux-x86_64) ASSET="pi-linux-amd64-static" ;;
  Linux-aarch64|Linux-arm64) ASSET="pi-linux-arm64-static" ;;
  *) echo "unsupported platform: $(uname -s)-$(uname -m)" >&2; exit 1 ;;
esac

URL="https://github.com/$REPO/releases/latest/download/$ASSET"
TMP="$(mktemp)"

trap 'rm -f "$TMP"' EXIT

echo "downloading $URL"
curl -fL --retry 3 -o "$TMP" "$URL"

if file "$TMP" | grep -q "statically linked"; then
  mkdir -p "$PREFIX"
  install -m 0755 "$TMP" "$PREFIX/pi"
  echo "installed static pi -> $PREFIX/pi"
  "$PREFIX/pi" --info | head -2
else
  echo "error: downloaded file is not a static binary (release not built yet?)" >&2
  exit 1
fi
