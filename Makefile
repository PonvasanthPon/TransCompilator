CARGO ?= cargo

SANDBOX_SUFFIX := box
FORBIDDEN_REPO := rbmt-sand$(SANDBOX_SUFFIX)
HOME_STEM := /home/ponvasan
FORBIDDEN_HOME := $(HOME_STEM)th
FORBIDDEN_REFS := $(FORBIDDEN_REPO)|$(FORBIDDEN_HOME)

.PHONY: build release test lint fmt-check audit selftest editor lsp clean

build:
	$(CARGO) build --workspace

release:
	$(CARGO) build --workspace --release

test: build
	$(CARGO) test --workspace

lint:
	$(CARGO) clippy --workspace --all-targets -- -D warnings

fmt-check:
	$(CARGO) fmt --all -- --check

selftest: build
	./target/debug/tc --selftest

editor: build
	$(CARGO) run --bin r1-editor --

lsp: build
	$(CARGO) run --bin tc-lsp -- --stdio

audit:
	@echo "tc_r1 audit"
	@ml="$$(find . \( -path ./.git -o -path ./target \) -prune -o -type f \( -name '*.onnx' -o -name '*.pt' -o -name '*.binlm' -o -name '*.mem' -o -name '*.hmm' -o -name '*.gguf' -o -name '*.safetensors' \) -print)"; \
	if [ -n "$$ml" ]; then echo "  FAIL: ML/model files found:"; echo "$$ml"; exit 1; fi; \
	echo "  ML/model files                      : none"
	@bad="$$(awk '/^\[dependencies\]/ { deps = 1; next } /^\[/ { deps = 0 } deps && NF && $$0 !~ /^[[:space:]]*#/ && $$0 !~ /path[[:space:]]*=/ { print FILENAME ": " $$0 }' Cargo.toml crates/*/Cargo.toml 2>/dev/null || true)"; \
	if [ -n "$$bad" ]; then echo "  FAIL: non-path dependency entries:"; echo "$$bad"; exit 1; fi; \
	echo "  workspace dependencies              : local path only"
	@ext=""; \
	for pkg in tc-ir tc-codegen-x86 tc-typeck tc-lexicon tc-syntax tc-semantics tc-cli tc-editor tc-lsp; do \
		out="$$($(CARGO) tree -p $$pkg --edges normal --prefix none 2>/dev/null | awk 'NF { print $$1 }' | grep -v '^tc-' || true)"; \
		ext="$$ext$$out"; \
	done; \
	ext="$$(echo "$$ext" | sort -u | sed '/^$$/d')"; \
	if [ -n "$$ext" ]; then echo "  FAIL: external crates in workspace crates:"; echo "$$ext"; exit 1; fi; \
	echo "  workspace external crates           : 0"
	@names="$$(grep '^name = ' Cargo.lock | sed 's/name = "//; s/"//' | sort)"; \
	expected="tc-cli tc-codegen-x86 tc-editor tc-ir tc-lexicon tc-lsp tc-semantics tc-syntax tc-typeck"; \
	if [ "$$names" != "$$(printf '%s\n' $$expected | sort)" ]; then echo "  FAIL: Cargo.lock must contain exactly the 9 local tc-* packages"; echo "$$names"; exit 1; fi; \
	if grep -q '^source = ' Cargo.lock; then echo "  FAIL: registry/git source entries found in Cargo.lock"; exit 1; fi; \
	if grep -q '^checksum = ' Cargo.lock; then echo "  FAIL: checksum entries found in Cargo.lock"; exit 1; fi; \
	if grep -qE '^name = "(eframe|egui|serde|serde_json|tokio)"' Cargo.lock; then echo "  FAIL: external runtime crate found in Cargo.lock"; exit 1; fi; \
	echo "  Cargo.lock packages                 : 9 local tc-* only (0 sources, 0 checksums)"
	@web="$$(grep -R -n -E 'TcpListener|frontend\.html|xdg-open|text/html|<html|http://|https://' --exclude-dir=generated crates/*/src 2>/dev/null || true)"; \
	if [ -n "$$web" ]; then echo "  FAIL: HTTP/HTML/browser code in workspace crates:"; echo "$$web"; exit 1; fi; \
	echo "  network/HTML/browser references     : none"
	@refs="$$(grep -R -n -E '$(FORBIDDEN_REFS)' crates std Makefile Cargo.toml 2>/dev/null || true)"; \
	if [ -n "$$refs" ]; then echo "  FAIL: forbidden path references:"; echo "$$refs"; exit 1; fi; \
	echo "  forbidden path references           : none"
	@echo "tc_r1 audit: PASS"

clean:
	$(CARGO) clean
