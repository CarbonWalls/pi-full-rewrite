#!/usr/bin/env bash
# Bundle a project's `.pi/` config (pi.json, skills/, templates/, ...) into a
# ready-to-share tarball (default ./pi-package.tar.gz).
# Install on another machine with:  pi --install-package pi-package.tar.gz
set -euo pipefail
ROOT="${1:-.}"
OUT="${2:-pi-package.tar.gz}"
case "$OUT" in
  /*) TARGET="$OUT" ;;
  *)  TARGET="$(pwd)/$OUT" ;;
esac
[ -d "$ROOT/.pi" ] || { echo "no .pi/ under $ROOT — run pi --init first"; exit 1; }
cd "$ROOT"
ARGS=()
for e in .pi/pi.json .pi/skills .pi/templates .pi/themes .pi/extensions .pi/packages; do
  [ -e "$e" ] && ARGS+=("$e")
done
tar -czf "$TARGET" "${ARGS[@]}"
echo "wrote $OUT (install with: pi --install-package $OUT)"
