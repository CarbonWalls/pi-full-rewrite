#!/usr/bin/env bash
# Release sign-off: run the whole pi verification matrix and report PASS/FAIL.
# Everything runs against the local tree (no installs elsewhere).
# HTTP checks use python3 (no curl dependency).
set -uo pipefail
cd "$(dirname "$0")/.."
export RUSTUP_HOME="$PWD/.rustup" CARGO_HOME="$PWD/.cargo-cache"
R="${R:-target/release/pi}"
[ -x "$R" ] || R="dist/pi"
[ -x "$R" ] || R="./pi"
[ -x "$R" ] || { echo "release binary missing — run ./scripts/release.sh first"; exit 1; }
S=$(mktemp -d /tmp/pi-verify.XXXXXX)
PASS=0; FAIL=0

ok()   { PASS=$((PASS+1)); echo "  PASS  $1"; }
bad()  { FAIL=$((FAIL+1)); echo "  FAIL  $1"; }
check(){ if eval "$2"; then ok "$1"; else bad "$1"; fi; }

hget() {
  python3 - "$1" <<'PYEOF'
import sys, urllib.request
try:
    sys.stdout.write(urllib.request.urlopen(sys.argv[1], timeout=5).read().decode(errors="replace"))
except Exception:
    pass
PYEOF
}

# Decode a `script -c` capture into the final on-screen frame (handles the
# diff renderer's per-character CUP writes), then you grep the plain text.
pty_screen() {
  python3 - "$1" <<'PYEOF'
import re, sys
raw = open(sys.argv[1], "rb").read()
# stop at the final alternate-screen exit so the last live frame is shown
marker = raw.rfind(b"\x1b[?1049l")
if marker >= 0:
    raw = raw[:marker]
grid = [[" " for _ in range(240)] for _ in range(80)]
r = c = 0; i = 0; n = len(raw)
def put(ch):
    global r, c
    if r < 80 and c < 240: grid[r][c] = ch
    c += 1
while i < n:
    b = raw[i]
    if b == 0x1b and i + 1 < n and raw[i + 1] == ord("["):
        m = re.match(rb"([0-9;?]*)([A-Za-z])", raw[i + 2:i + 16])
        if m:
            body = m.group(1).decode(); code = m.group(2).decode()
            i += 2 + len(m.group(0))
            if code == "H" and body:
                p = body.split(";")
                r = int(p[0]) - 1; c = int(p[1] if len(p) > 1 else "1") - 1
            continue
        i += 2; continue
    elif b == 0x0d: c = 0; i += 1; continue
    elif b == 0x0a: r += 1; c = 0; i += 1; continue
    elif b == 0x08: c = max(0, c - 1); i += 1; continue
    else: put(chr(b)); i += 1
sys.stdout.write("\n".join("".join(row).rstrip() for row in grid))
PYEOF
}

echo "== pi release sign-off ($R) =="

check "version"              '$("$R" --version >/dev/null 2>&1)'
check "info"                 '$("$R" --info | grep -q "models")'
check "print turn"           'echo hi | "$R" --quiet --root "$S" --session-dir "$S/se" >/dev/null 2>&1'
check "json events"          'echo hi | "$R" --json --root "$S" --session-dir "$S/sj" 2>/dev/null | grep -q "type"'
check "stats"                'f=$(ls -t "$S/sj"/*.jsonl 2>/dev/null | head -1); [ -n "$f" ] && "$R" --stats "$f" >/dev/null 2>&1'
check "export md"            'f=$(ls -t "$S/sj"/*.jsonl 2>/dev/null | head -1); "$R" --export "$f" --format md | grep -q "# session"'
check "export json"          'f=$(ls -t "$S/sj"/*.jsonl 2>/dev/null | head -1); "$R" --export "$f" --format json | grep -q "messages"'
check "sessions json"        '"$R" --sessions --json --session-dir "$S/sj" | grep -q "sessions"'
check "doctor"               '"$R" --doctor --root "$S" >/dev/null 2>&1'
check "list-models"          '"$R" --list-models --root "$S" | grep -q "mock/echo"'
check "completion bash"      '"$R" --completion bash | grep -q "_pi"'
check "completion fish"      '"$R" --completion fish | grep -q "complete -c pi"'
check "mcp-server init"      'echo "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\"}" | "$R" --mcp-server --root "$S" 2>/dev/null | grep -q "serverInfo"'
check "rpc protocol"         'printf "{\"type\":\"tools\"}\n{\"type\":\"quit\"}\n" | "$R" --rpc --root "$S" 2>/dev/null | grep -q "tools_result"'

# serve + endpoints + graceful SIGTERM
PORT=8799
"$R" --serve --port $PORT --root "$S" --session-dir "$S/sz" >/dev/null 2>&1 &
SPID=$!
sleep 0.9
check "serve healthz"        'hget http://127.0.0.1:$PORT/healthz | grep -q ok'
check "serve dashboard"      'hget http://127.0.0.1:$PORT/ | grep -q "<!doctype html>"'
check "serve cost endpoint"  'hget http://127.0.0.1:$PORT/cost | grep -q model_calls'
check "serve metrics"        'hget http://127.0.0.1:$PORT/metrics | grep -q pi_requests'
check "serve rpc"            'printf "{\"type\":\"quit\"}\n" | python3 -c "
import sys, urllib.request
req = urllib.request.Request(sys.argv[1], data=sys.stdin.buffer.read(), method=\"POST\")
urllib.request.urlopen(req, timeout=5).read()
" http://127.0.0.1:$PORT/rpc >/dev/null 2>&1'
check "serve HEAD"             'python3 -c "
import http.client
c = http.client.HTTPConnection(\"127.0.0.1\", $PORT, timeout=5)
c.request(\"HEAD\", \"/healthz\")
r = c.getresponse()
assert r.status == 200
assert r.read() == b\"\"
" >/dev/null 2>&1'
check "serve sigterm"        'kill -TERM $SPID 2>/dev/null && sleep 0.5 && ! kill -0 $SPID 2>/dev/null'
printf '{"type":"quit"}\n' | python3 -c "
import sys, urllib.request
req = urllib.request.Request('http://127.0.0.1:$PORT/rpc', data=sys.stdin.buffer.read(), method='POST')
urllib.request.urlopen(req, timeout=5).read()
" >/dev/null 2>&1
# access log (status-coded) on its own server instance
"$R" --serve --access-log --port 8801 --root "$S" --session-dir "$S/al" --no-session >/dev/null 2>&1 &
ALP=$!
sleep 0.7
hget http://127.0.0.1:8801/healthz >/dev/null
if hget http://127.0.0.1:8801/access-log | grep -q "200 GET /healthz"; then
  ok "serve access-log"
else
  bad "serve access-log"
fi
kill -TERM $ALP 2>/dev/null

# TUI under a PTY: turn + command + clean exit + terminal restore
TUI=$(mktemp /tmp/pi-tui.XXXXXX)
{ printf 'hello tui\n'; sleep 0.9; printf '/find hello\n'; sleep 0.9; printf '/exit\n'; } | timeout 25 script -qec \
  "stty cols 100 rows 25; $R -i --root $S --session-dir $S/tu --model mock/echo" /dev/null >"$TUI" 2>&1
check "tui transcript content" 'pty_screen "$TUI" | grep -q "echo: hello tui"'
check "tui find hits"          'pty_screen "$TUI" | grep -q "find: 2"'
if [ -f "$TUI" ] && ! grep -q panicked "$TUI"; then
  ok "tui clean exit"
else
  bad "tui clean exit"
fi
if [ -f "$TUI" ] && grep -qF "$(printf '\033[?1049l')" "$TUI"; then
  ok "tui terminal restore"
else
  bad "tui terminal restore"
fi
rm -f "$TUI"

# user journey: init -> configure -> package -> install into a 2nd root
TOUR_OUT=$(scripts/tour.sh 2>&1)
if echo "$TOUR_OUT" | grep -q "== tour: 12 passed, 0 failed =="; then
  ok "user journey tour (12/12)"
else
  bad "user journey tour"; echo "$TOUR_OUT" | tail -6
fi

# installer: quiet by default — never edits shell profiles (round 28).
# CI has no dist/pi, so stage one from the release binary (save/restore any
# pre-existing dist/pi so this stays non-destructive).
IVHOME=$(mktemp -d /tmp/pi-insthome.XXXXXX)
IPT=$(mktemp -d /tmp/pi-instprefix.XXXXXX)
HAD_DIST=0
if [ -f dist/pi ]; then cp dist/pi /tmp/pi-dist-save.$$; HAD_DIST=1; fi
mkdir -p dist && cp "$R" dist/pi
INS_OUT=$(HOME="$IVHOME" PREFIX="$IPT" scripts/install.sh 2>&1 || true)
if [ "$HAD_DIST" = "1" ]; then mv -f /tmp/pi-dist-save.$$ dist/pi; else rm -f dist/pi; fi
if echo "$INS_OUT" | grep -q "installed pi to $IPT/pi"    && [ ! -f "$IVHOME/.profile" ] && [ ! -f "$IVHOME/.bashrc" ]; then
  ok "installer quiet (no profile edits by default)"
else
  bad "installer quiet"; echo "$INS_OUT" | tail -4
fi
rm -rf "$IVHOME" "$IPT"

# static musl, if present
MS=target/aarch64-unknown-linux-musl/release/pi
if [ -x "$MS" ]; then
  check "static binary runs" 'echo hi | "$MS" --version >/dev/null 2>&1'
fi

rm -rf "$S"
echo
echo "== result: $PASS passed, $FAIL failed =="
[ "$FAIL" -eq 0 ]
