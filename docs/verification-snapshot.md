# Verification snapshot — v1.0.7 era (2026-07-16)

The exact state verified for the current release:

| gate | result |
|---|---|
| Unit + integration tests | **133 / 133** |
| Clippy (all targets) | **0 warnings** |
| Release sign-off (`scripts/verify.sh`) | **31 / 31** |
| User journey (`scripts/tour.sh`) | **12 / 12** |
| GitHub Actions CI (`ci.yml`, pinned 1.99.0) | **passing** (test + static-musl jobs) |
| Commit hygiene | no `target/`, `dist/`, `.rustup/`, `.pi/` tracked (CI-enforced) |

## Cross-arch acceptance (real amd64 hardware, shipped binaries)
- The complete `scripts/verify.sh` ran on a Debian/WSL x86_64 box against the
  released static binary: **28/28 applicable checks pass** (the only skip is
  the local-musl-gated "static binary runs").
- One-line install accepted end-to-end (incl. the no-`file(1)` fallback),
  serve E2E (healthz/rpc/sessions), and an offline agent turn all verified.
- amd64 numbers (current release): `--version` 2 ms, mock turn 6 ms, serve
  RSS 3.3 MB flat over 20 s.

## Releases
- v1.0.0 … v1.0.7 — each verified: assets download and run
  (`--info` reports the tagging commit via `PI_BUILD_SHA`).
- Per-tag assets: `pi-linux-amd64` (glibc), `pi-linux-amd64-static` (musl),
  README, VERSION. arm64-static is a maintainer-aarch64-host build.

## Benchmarks (release binary, real hardware)
- aarch64 dev host: `--version` ~15–19 ms; full mock turn ~24 ms (current binary).
- x86_64 Debian/WSL (Ryzen AI 7): `--version` **2 ms**; mock turn **6 ms**.

## Notes for the next maintainer
- Runners ship no git identity and are surprising in other ways: tests set
  identity hermetically; CI failures surface via `::error::` annotations
  (checked via the public check-runs API when logs need auth).
- The bash timeout path reaps the descendant tree BEFORE killing the shell
  (reparenting otherwise strands children); regression-tested.

Re-verify each claim by running `./build.sh test`, `./build.sh clippy
--all-targets`, `./scripts/verify.sh`, `./scripts/tour.sh`.
