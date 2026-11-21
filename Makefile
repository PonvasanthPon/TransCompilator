CARGO ?= cargo

SANDBOX_SUFFIX := box
FORBIDDEN_REPO := rbmt-sand$(SANDBOX_SUFFIX)
HOME_STEM := /home/ponvasan
FORBIDDEN_HOME := $(HOME_STEM)th
FORBIDDEN_REFS := $(FORBIDDEN_REPO)|$(FORBIDDEN_HOME)

.PHONY: build release test lint fmt-check audit selftest editor editor-gui clean

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

editor-gui: build
	$(CARGO) run --bin r1-editor-gui --

audit:
	@echo "tc_r1 audit"
	@ml="$$(find . \( -path ./.git -o -path ./target \) -prune -o -type f \( -name '*.onnx' -o -name '*.pt' -o -name '*.binlm' -o -name '*.mem' -o -name '*.hmm' -o -name '*.gguf' -o -name '*.safetensors' \) -print)"; \
	if [ -n "$$ml" ]; then echo "  FAIL: ML/model files found:"; echo "$$ml"; exit 1; fi; \
	echo "  ML/model files                      : none"
	@bad="$$(awk '/^\[dependencies\]/ { deps = 1; next } /^\[/ { deps = 0 } deps && NF && $$0 !~ /^[[:space:]]*#/ && $$0 !~ /path[[:space:]]*=/ { print FILENAME ": " $$0 }' Cargo.toml crates/*/Cargo.toml 2>/dev/null | grep -v '^crates/tc-editor-gui/Cargo.toml:' || true)"; \
	if [ -n "$$bad" ]; then echo "  FAIL: non-path dependency entries:"; echo "$$bad"; exit 1; fi; \
	echo "  compiler/editor dependencies        : local path only"
	@ext=""; \
	for pkg in tc-ir tc-codegen-x86 tc-typeck tc-lexicon tc-syntax tc-semantics tc-cli tc-editor; do \
		out="$$($(CARGO) tree -p $$pkg --edges normal --prefix none 2>/dev/null | awk 'NF { print $$1 }' | grep -v '^tc-' || true)"; \
		ext="$$ext$$out"; \
	done; \
	ext="$$(echo "$$ext" | sort -u | sed '/^$$/d')"; \
	if [ -n "$$ext" ]; then echo "  FAIL: external crates in compiler/editor crates:"; echo "$$ext"; exit 1; fi; \
	echo "  compiler crates external crates     : 0"
	@gui_bad="$$(awk '/^\[dependencies\]/ { deps = 1; next } /^\[/ { deps = 0 } deps && NF && $$0 !~ /^[[:space:]]*#/ { print }' crates/tc-editor-gui/Cargo.toml | grep -v -E '^(eframe|tc-[a-z0-9-]+) =' || true)"; \
	if [ -n "$$gui_bad" ]; then echo "  FAIL: GUI dependency outside eframe/tc-*:"; echo "$$gui_bad"; exit 1; fi; \
	if ! grep -q '^eframe = ' crates/tc-editor-gui/Cargo.toml; then echo "  FAIL: native GUI must depend on eframe"; exit 1; fi; \
	echo "  GUI dependencies                    : eframe + local tc-* only"
	@web="$$(grep -R -n -E 'TcpListener|frontend\.html|xdg-open|chromium|google-chrome|brave-browser|microsoft-edge|text/html|<html|http://|https://' crates/tc-editor-gui/src 2>/dev/null || true)"; \
	if [ -n "$$web" ]; then echo "  FAIL: HTTP/HTML/browser code in native GUI:"; echo "$$web"; exit 1; fi; \
	echo "  GUI HTTP/HTML/browser references    : none"
	@if ! grep -q 'name = "eframe"' Cargo.lock; then echo "  FAIL: eframe missing from Cargo.lock"; exit 1; fi; \
	pkgs="$$(grep -c '^name = ' Cargo.lock)"; \
	if [ "$$pkgs" -lt 9 ]; then echo "  FAIL: expected at least 9 packages in Cargo.lock, found $$pkgs"; exit 1; fi; \
	echo "  Cargo.lock packages                 : $$pkgs total (native GUI graph included)"
	@refs="$$(grep -R -n -E '$(FORBIDDEN_REFS)' crates std Makefile Cargo.toml 2>/dev/null || true)"; \
	if [ -n "$$refs" ]; then echo "  FAIL: forbidden path references:"; echo "$$refs"; exit 1; fi; \
	echo "  forbidden path references           : none"
	@echo "tc_r1 audit: PASS"

clean:
	$(CARGO) clean
