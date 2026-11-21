CARGO ?= cargo

SANDBOX_SUFFIX := box
FORBIDDEN_REPO := rbmt-sand$(SANDBOX_SUFFIX)
HOME_STEM := /home/ponvasan
FORBIDDEN_HOME := $(HOME_STEM)th
FORBIDDEN_REFS := $(FORBIDDEN_REPO)|$(FORBIDDEN_HOME)

.PHONY: build release test lint fmt-check audit selftest clean

build:
	$(CARGO) build --workspace

release:
	$(CARGO) build --workspace --release

test:
	$(CARGO) test --workspace

lint:
	$(CARGO) clippy --workspace --all-targets -- -D warnings

fmt-check:
	$(CARGO) fmt --all -- --check

selftest: build
	./target/debug/tc --selftest

audit:
	@echo "tc_r1 audit"
	@ml="$$(find . \( -path ./.git -o -path ./target \) -prune -o -type f \( -name '*.onnx' -o -name '*.pt' -o -name '*.binlm' -o -name '*.mem' -o -name '*.hmm' -o -name '*.gguf' -o -name '*.safetensors' \) -print)"; \
	if [ -n "$$ml" ]; then echo "  FAIL: ML/model files found:"; echo "$$ml"; exit 1; fi; \
	echo "  ML/model files                      : none"
	@bad="$$(awk '/^\[dependencies\]/ { deps = 1; next } /^\[/ { deps = 0 } deps && NF && $$0 !~ /^[[:space:]]*#/ && $$0 !~ /path[[:space:]]*=/ { print FILENAME ": " $$0 }' Cargo.toml crates/*/Cargo.toml)"; \
	if [ -n "$$bad" ]; then echo "  FAIL: non-path dependency entries:"; echo "$$bad"; exit 1; fi; \
	echo "  non-path dependencies               : none"
	@if grep -q 'source = "registry+' Cargo.lock; then echo "  FAIL: registry source found in Cargo.lock"; exit 1; fi; \
	if grep -q '^checksum = ' Cargo.lock; then echo "  FAIL: checksummed external package found in Cargo.lock"; exit 1; fi; \
	pkgs="$$(grep -c '^name = ' Cargo.lock)"; \
	if [ "$$pkgs" -ne 7 ]; then echo "  FAIL: expected 7 workspace packages in Cargo.lock, found $$pkgs"; exit 1; fi; \
	echo "  Cargo.lock packages                 : 7 local (0 external)"
	@unexpected="$$(grep '^name = ' Cargo.lock | grep -v -E '^name = "tc-(ir|codegen-x86|typeck|lexicon|syntax|semantics|cli)"$$' || true)"; \
	if [ -n "$$unexpected" ]; then echo "  FAIL: unexpected packages in Cargo.lock:"; echo "$$unexpected"; exit 1; fi; \
	echo "  Cargo.lock identities               : 7 tc-* only"
	@refs="$$(grep -R -n -E '$(FORBIDDEN_REFS)' crates std Makefile Cargo.toml 2>/dev/null || true)"; \
	if [ -n "$$refs" ]; then echo "  FAIL: forbidden path references:"; echo "$$refs"; exit 1; fi; \
	echo "  forbidden path references           : none"
	@echo "tc_r1 audit: PASS"

clean:
	$(CARGO) clean
