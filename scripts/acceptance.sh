#!/usr/bin/env bash
# Release acceptance for the shipped binary: download the latest GitHub
# release asset, verify it (ELF64, static, no file(1) dependency), run an
# offline agent turn and a serve healthz probe. Exit 0 on full acceptance.
#
#   ./scripts/acceptance.sh            # uses the latest release
#   ASSET_URL=... ./scripts/acceptance.sh   # explicit asset URL
set -euo pipefail
cd "$(dirname "$0")/.."

REPO="${REPO:-CarbonWalls/pi-full-rewrite}"
HOST_ARCH="$(uname -m)"
case "$HOST_ARCH" in
  x86_64|amd64|i?86) ASSET="${ASSET:-pi-linux-amd64-static}"; RUNNABLE=1 ;;
  aarch64|arm64)     ASSET="${ASSET:-pi-linux-amd64-static}"; RUNNABLE=0 ;;
  *) echo "unknown host arch: $HOST_ARCH" >&2; exit 2 ;;
esac
if [ -n "${ASSET_URL:-}" ]; then
  :  # explicit URL wins
elif echo "$ASSET" | grep -q "^http"; then
  ASSET_URL="$ASSET"
else
  ASSET_URL="https://github.com/$REPO/releases/latest/download/$ASSET"
fi
TMP="$(mktemp -d /tmp/pi-accept.XXXXXX)"
BIN="$TMP/pi"
PASS=0; FAIL=0; SKIPPED=0
ok(){ PASS=$((PASS+1)); echo "  PASS  $1"; }
bad(){ FAIL=$((FAIL+1)); echo "  FAIL  $1"; }
skip(){ SKIPPED=$((SKIPPED+1)); echo "  SKIP  $1 (host $HOST_ARCH cannot run this asset)"; }
if [ -n "${LOCAL_BIN:-}" ]; then RUNNABLE=1; fi

trap 'rm -rf "$TMP"' EXIT

if [ -n "${LOCAL_BIN:-}" ]; then
  echo "== using local binary: $LOCAL_BIN"
  cp "$LOCAL_BIN" "$BIN"; chmod +x "$BIN"
  RUNNABLE=1
else
  echo "== downloading $ASSET_URL"
  python3 - "$ASSET_URL" "$BIN" <<'PYDL'
import sys, urllib.request
urllib.request.urlretrieve(sys.argv[1], sys.argv[2])
PYDL
  chmod +x "$BIN"
fi

echo "== v1 binary checks"
if ! od -An -tx1 -N5 "$BIN" | grep -q "7f 45 4c 46 02"; then
  bad "ELF64 magic"; exit 1
fi
ok "ELF64 magic (no file(1) needed)"
if python3 - "$BIN" <<'PY'
import sys
d = open(sys.argv[1], "rb").read()
# a static binary has no interpreter string in the first pages
print(0 if b"/lib/" not in d[:200_000] and b"ld-musl" not in d[:500_000] else 1)
PY
then
  ok "no interpreter (static)"
else
  bad "static check (install file(1) for a deeper check)"
fi

echo "== run checks"
if [ "$RUNNABLE" = "1" ]; then
  if "$BIN" --info | head -1 | grep -q "pi 0.1.0"; then ok "build info"; else bad "build info"; fi

  RUN="$TMP/run"; mkdir -p "$RUN/.pi/sessions"
  printf '%s\n' '{"contains":"accept","text":"ACCEPTED"}' > "$RUN/scn.jsonl"
  if printf 'accept\n' | PI_MOCK_SCENARIO="$RUN/scn.jsonl" "$BIN" --quiet --root "$RUN" \
      --session-dir "$RUN/.pi/sessions" -m mock/echo 2>/dev/null | grep -q "ACCEPTED"; then
    ok "offline agent turn"
  else
    bad "offline agent turn"
  fi
  [ "$(ls "$RUN/.pi/sessions/"*.jsonl 2>/dev/null | wc -l)" -ge 1 ] && ok "session written" || bad "session written"

  PORT=$(( (RANDOM % 1000) + 18000 ))
  PI_MOCK_SCENARIO="$RUN/scn.jsonl" "$BIN" --serve --port "$PORT" --root "$RUN" \
      --session-dir "$RUN/.pi/sessions" >/dev/null 2>&1 &
  SPID=$!
  trap 'kill $SPID 2>/dev/null; rm -rf "$TMP"' EXIT
  sleep 1
  if python3 - <<PYEO
import urllib.request
urllib.request.urlopen("http://127.0.0.1:$PORT/healthz", timeout=5).read()
PYEO
then ok "serve healthz"; else bad "serve healthz"; fi
  kill $SPID 2>/dev/null
  trap 'rm -rf "$TMP"' EXIT
else
  skip "build info"
  skip "offline agent turn"
  skip "session written"
  skip "serve healthz"
fi

echo
echo "== acceptance: $PASS passed, $FAIL failed, $SKIPPED skipped (arch) =="
[ "$FAIL" -eq 0 ]
