# Releasing pi-rs

Everything runs from this directory; nothing is installed elsewhere.

## 1. Build

```
./build.sh build --release -p pi-coding-agent        # dynamic release
# static musl (host with a musl cross clang; see README "static musl"):
./build.sh build --release --target aarch64-unknown-linux-musl -p pi-coding-agent
```

## 2. Bundle

```
./scripts/release.sh        # → dist/ (pi, pi-static-arm64, README, docs, examples, scripts)
```

## 3. Sign off

```
./scripts/verify.sh         # 29-point matrix: modes, protocols, serve,
                            #   TUI (PTY, frame-decoded), user-journey tour,
                            #   static binary. Must print
                            #   "29 passed, 0 failed".
./build.sh test             # 131 tests across 6 crates
./build.sh clippy           # zero warnings
```

## 4. Distribute

- `dist/` — copy the bundle anywhere; both binaries are self-contained
  (static needs no loader at all).
- Docker: `cp dist/pi-static-arm64 . && docker build -t pi .` (scratch image).
- Install: `./scripts/install.sh` (installs `dist/pi` to `~/.local/bin`).
- Share configs: `scripts/package.sh <proj> <out.tar.gz>` → `pi --install-package`.

## 5. Health

- `pi --doctor` — environment checks (binaries, dir, git, disk, keys, config,
  session dir, session size).
- `pi --info` — build banner (version, model/tool counts, target).

## Current release state (round 64)

- 131 tests green, clippy 0, sign-off 28/28, `--version` ≈ 13 ms,
  release 4.6 MB, static 4.7 MB.
- Full history: `PROGRESS.md`, `CHANGELOG.md`.
