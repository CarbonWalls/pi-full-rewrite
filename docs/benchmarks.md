# Benchmarks

Measured on the release build (`aarch64` host, LTO + `panic=abort`, `strip`).
Command: `./build.sh build --release -p pi-coding-agent` (and the musl target
for the static binary).

| Metric | pi (Rust) | reference pi (Node, `node dist/cli.js`) | speedup |
|---|---|---|---|
| `pi --version` (cold start) | **~13 ms** | 11.6–14.4 s | **~900×** |
| `pi --help` | **~14 ms** | ~28 s | **~1 900×** |
| Full mock agent turn (print mode, incl. process start + session persist) | **~20 ms** | — | — |
| Idle `pi serve` memory (RSS) | **~1.9 MB** | ~43 MB | **~22× smaller** |
| Binary size (release) | ~4.6 MB | — | — |
| Static musl (aarch64) | ~4.4 MB, `ldd` → *not a dynamic executable* | — | — |

Notes

- Because the whole runtime is one native binary with an embedded model
  catalog, there is no Node startup, no module resolution, and no V8 warm-up.
- `pi serve` keeps a single long-lived process; per-turn latency over HTTP is
  dominated by the provider round-trip.
- The static musl build runs on any Linux without shared-library
  dependencies (no glibc/Node/cargo needed at deploy time).
