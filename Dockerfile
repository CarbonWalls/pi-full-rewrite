# `pi serve` as a scratch (zero-dependency) container.
#
# The image contains ONLY the statically-linked musl binary produced by
# scripts/release.sh — no OS userspace, no libc, nothing else.
#
# Build:
#   scripts/release.sh                      # produces dist/pi-static-arm64
#   cp dist/pi-static-arm64 ./pi-static-arm64
#   docker build -t pi .
#
# Run:
#   docker run --rm -p 8787:8787 pi
#   # browse http://localhost:8787/ (embedded dashboard)

FROM scratch
COPY pi-static-arm64 /usr/bin/pi
WORKDIR /work
EXPOSE 8787
ENTRYPOINT ["/usr/bin/pi"]
CMD ["--serve", "--port", "8787", "--root", "/work", "--no-session"]
