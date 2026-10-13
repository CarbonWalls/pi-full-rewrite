# pi-rs — common developer entry points.
#
# Everything is sandboxed to this directory (see build.sh). On machines with
# a system Rust toolchain (e.g. CI) the same targets work transparently.

BIN  := pi-coding-agent

.PHONY: build test clippy verify tour bench release static dist info commit

build:            ## Debug build of the pi binary
	./build.sh build -p $(BIN)

test:             ## Full test suite (122+ tests, offline)
	./build.sh test

clippy:           ## Lint lib + bins + tests (must be 0 warnings)
	./build.sh clippy --all-targets

verify:           ## 28-point release sign-off
	./scripts/verify.sh

tour:             ## 12-step fresh-user journey
	./scripts/tour.sh

bench:            ## Spot-measure startup + a mock turn
	@./target/release/pi --version >/dev/null && \
	echo "pi --version ok (see docs/benchmarks.md for the full table)"

static:           ## Static musl binary (host must have a musl clang)
	./build.sh build --release --target "$$(rustc -vV | sed -n 's/host: //p')" -p $(BIN) || \
	./build.sh build --release -p $(BIN)

release:          ## Build release + static, bundle into dist/, sign off
	./scripts/release.sh
	./scripts/verify.sh

dist:             ## Just refresh the dist/ bundle
	./scripts/release.sh

info:             ## Build banner + doctor
	./target/release/pi --info
	./target/release/pi --doctor

commit:           ## Commit the current state with round changelog
	git add -A && git commit -m "$(if $(m),$(m),WIP)"
