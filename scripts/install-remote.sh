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

if command -v file >/dev/null 2>&1; then
  if ! file "$TMP" | grep -qiE "statically linked|static-pie linked"; then
    echo "error: downloaded binary does not look static: $(file "$TMP")" >&2
    exit 1
  fi
elif od -An -tx1 -N5 "$TMP" | grep -qE "7f 45 4c 46 02"; then
  # file(1) missing: verify ELF64 magic (7f ELF, class=02) as a best effort
  echo "note: file(1) not installed; verified ELF64 magic only"
else
  echo "error: downloaded file is not an ELF64 binary (install file(1) for a deeper check)" >&2
  exit 1
fi

mkdir -p "$PREFIX"
install -m 0755 "$TMP" "$PREFIX/pi"
echo "installed static pi -> $PREFIX/pi"
"$PREFIX/pi" --info | head -2
