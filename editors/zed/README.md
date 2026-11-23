# Zed setup for TransCompilator English

This directory is fully offline, zero-AI, and never signs in.

1. Install Zed with the official installer or a distro package:
   `curl -f https://zed.dev/install.sh | sh`
   Do not sign in or enable collaboration.
2. Run the safe, non-destructive helper:
   `./editors/zed/install_zed_config.sh`
   It builds `tc`, `tc-lsp`, and `r1-editor` in release mode, symlinks them
   into `~/.local/bin`, backs up any existing Zed settings, and merges the
   offline zero-AI configuration. It never modifies `~/.config/zed` during
   tests; you must run it yourself.
3. Open `/home/ponvasanth/tc_r1` in Zed, choose
   `Extensions -> Install Dev Extension`, and select
   `editors/zed/tc-eng-extension`. Open any `.eng` file. You get live
   diagnostics, hover, completion, go-to-definition (including `std/*.eng`),
   an Outline panel, semantic tokens, and the five `tc:` tasks.

## Monochrome typographic highlighting

`.eng` files use language-scoped LSP semantic tokens instead of rainbow
C/Python syntax colors. The rules live in
`tc-eng-extension/languages/eng/semantic_token_rules.json` and apply only to
the `TransCompilator English` language:

- **Bold**: computational verbs, phrase heads, and control words.
- **Subtle underline** (`#c8c8cc`): variables, multi-word names, fields, and
  record targets.
- **Italic**: resolved ACE anaphors (`it`, `its`, `their`, `them`,
  `the result`) and string literals.
- **Normal weight**: numbers and mid-sentence copulas/prepositions.
- **Muted italic gray**: explicit comments and whole sentences classified by
  `tc-semantics` as `COMMENT_OMITTED` narrative prose.

English glue words (`the`, `of`, `a`, `an`, `by`, `me`, `out`) and punctuation
stay in normal body ink. After changing the extension, run
`Extensions -> Reinstall / Install Dev Extension`, and use
`editor: restart language server` if Zed was already open.

The `.zed/tasks.json` file is included so opening the repository exposes
`tc: Run`, `tc: Run + Parity Check`, `tc: Paraphrase`, `tc: Trace`, and
`tc: Build Static ELF64` immediately.

`tc-lsp` speaks JSON-RPC over stdio and has no network access. Launch it
manually with:

```bash
CARGO_NET_OFFLINE=true cargo build --release --bin tc-lsp
./target/release/tc-lsp --stdio
```
