#!/usr/bin/env bash
# Scripted pi demo: runs a small mixed session (chat + tools + commands) with
# the mock provider and prints the resulting transcript to stdout — safe to
# run anywhere, no keys, no network.
set -euo pipefail
cd "$(dirname "$0")/.."
R="${R:-target/release/pi}"
S=$(mktemp -d /tmp/pi-demo.XXXXXX)
trap 'rm -rf "$S"' EXIT

echo "$ $R --interactive (scripted demo, mock provider)"
printf 'hello pi\n/tools\n/cost\n/session\n/label demo\n/label-done 0\n/exit\n' \
  | timeout 20 script -qec \
      "stty cols 100 rows 25; $R -i --root $S --session-dir $S/d --model mock/echo" \
      /dev/null >"$S/out" 2>&1

echo "--- pi finished (exit $?) ---"
echo "session files: $(ls "$S/d"/*.jsonl 2>/dev/null | wc -l)"
echo "label persisted: $(grep -l demo "$S/d"/*.jsonl | wc -l)"
echo
echo "Tip: run an interactive session yourself with:  pi"
