#!/usr/bin/env bash
# Build a self-contained release bundle under dist/ (both binaries + docs).
# Everything stays inside this directory (no installs elsewhere).
set -euo pipefail
# stamp the exact git commit into the binaries
if [ -d .git ]; then
  export PI_BUILD_SHA="$(git rev-parse --short HEAD 2>/dev/null || echo dev)"
fi
cd "$(dirname "$0")/.."

DIST="${1:-dist}"
echo "==> building release binary"
./build.sh build --release -p pi-coding-agent

MUSL=target/aarch64-unknown-linux-musl/release/pi
if [ -f "$MUSL" ]; then
  echo "==> static musl binary already present at $MUSL"
  echo "    (rebuild it manually when needed: see README 'static musl')"
else
  echo "==> static musl binary not built yet; skipping (run the musl build)"
fi

rm -rf "$DIST"
mkdir -p "$DIST"
cp target/release/pi "$DIST/pi"
[ -f "$MUSL" ] && cp "$MUSL" "$DIST/pi-static-arm64"
for d in README.md LICENSE*; do
  [ -e "$d" ] && cp "$d" "$DIST/"
done
mkdir -p "$DIST/docs"
cp docs/*.md "$DIST/docs/"
mkdir -p "$DIST/examples"
[ -d examples ] && cp -r examples/. "$DIST/examples/"
mkdir -p "$DIST/scripts"
cp scripts/*.sh "$DIST/scripts/" 2>/dev/null || true

cat > "$DIST/VERSION" <<EOF
pi $(target/release/pi --version | awk '{print $2}')
$(target/release/pi --info | head -1)
EOF

echo "==> bundle ready:"
du -sh "$DIST"
ls -la "$DIST"
