# Third-Party Notices

`tc_r1` itself is MIT-licensed. The following third-party assets are used or
referenced by the repository.

## CMU Link Grammar 5.13.2

- **Location:** `vendor/link-grammar/`
- **License:** LGPL-2.1-or-later
- **Use:** vendored parser library linked by the Rust compiler front end.

## Open English WordNet (OEWN) 2025

- **Location:** raw data is read read-only from the goldmines sandbox; generated
  tables live in `crates/tc-lexicon/src/generated/`.
- **License:** CC-BY-4.0
- **Use:** provenance-backed WordNet lemmas, synsets, and semantic concepts.

## VerbNet 3.4

- **Location:** raw data is read read-only from the goldmines sandbox; generated
  tables live in `crates/tc-lexicon/src/generated/`.
- **License:** CC-BY-4.0
- **Use:** provenance-backed VerbNet classes, frames, and predicates.

## Zed Extension API (`zed_extension_api`)

- **Location:** `editors/zed/tc-eng-extension/` (standalone; not a `tc_r1`
  workspace member).
- **License:** Apache-2.0
- **Use:** Zed development-extension scaffold that launches `tc-lsp`.
