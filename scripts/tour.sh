#!/usr/bin/env bash
# Fresh-user journey with the release binary: init -> configure (skill + theme)
# -> run -> package -> install into a second root -> verify -> cost + doctor.
# Every step must pass; prints PASS/FAIL like verify.sh.
set -uo pipefail
cd "$(dirname "$0")/.."
R="${R:-target/release/pi}"
[ -x "$R" ] || { echo "run ./scripts/release.sh first"; exit 1; }
T=$(mktemp -d /tmp/pi-tour.XXXXXX)
PASS=0; FAIL=0
ok(){ PASS=$((PASS+1)); echo "  PASS  $1"; }
bad(){ FAIL=$((FAIL+1)); echo "  FAIL  $1"; }
chk(){ if eval "$2" >/dev/null 2>&1; then ok "$1"; else bad "$1"; fi; }

echo "== pi user journey =="
A="$T/a"; B="$T/b"
chk "init scaffolds .pi"           '"$R" --init --root "$A"'
chk "pi.json written"              '[ -f "$A/.pi/pi.json" ]'
# configure a skill + theme
mkdir -p "$A/.pi/skills" "$A/.pi/themes"
printf '# greet\n\nRun this to greet prettily.\n' > "$A/.pi/skills/welcome.md"
printf '{"accent":"#00ff88"}\n' > "$A/.pi/themes/mint.json"
chk "run a turn in A"               'echo hi | "$R" --root "$A" --session-dir "$A/.pi/sessions" -m mock/echo'
chk "turn persisted a session"      '[ -n "$(ls "$A/.pi/sessions"/*.jsonl 2>/dev/null | head -1)" ]'
# package the config, then install it into an empty B
chk "package the .pi"               'bash scripts/package.sh "$A" "$T/pkg.tar.gz"'
mkdir -p "$B"
chk "install into B"                '"$R" --install-package "$T/pkg.tar.gz" --root "$B"'
chk "skill landed in B"             '[ -f "$B/.pi/skills/welcome.md" ]'
chk "theme landed in B"             '[ -f "$B/.pi/themes/mint.json" ]'
chk "run in B uses packaged config" 'echo ok | "$R" --root "$B" --session-dir "$B/.pi/sessions" -m mock/echo'
# accounting + diagnostics on B (now has usage entries)
chk "doctor on B"                   '"$R" --doctor --root "$B" --session-dir "$B/.pi/sessions" 2>&1 | grep -q "cost"'
chk "cost on B"                     '"$R" --cost --session-dir "$B/.pi/sessions" | grep -q "model calls"'
chk "stats on B session"            'f=$(ls "$B/.pi/sessions"/*.jsonl | head -1); "$R" --stats "$f" | grep -q "usage:"'

rm -rf "$T"
echo
echo "== tour: $PASS passed, $FAIL failed =="
[ "$FAIL" -eq 0 ]
