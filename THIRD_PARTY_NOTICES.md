# Third-Party Notices

`tc_r1` itself is proprietary and All Rights Reserved; see `LICENSE`. The
following third-party assets are used or referenced by the repository. The
canonical license text for each asset governs its use.

## CMU Link Grammar 5.13.2

- **Location:** `vendor/link-grammar/`
- **License:** LGPL-2.1-or-later
- **License text:** `vendor/link-grammar/LICENSE`
- **Upstream:** <https://github.com/opencog/link-grammar>
- **Use:** unmodified vendored parser library linked dynamically by the Rust
  compiler front end; it is not linked into the static zero-libc output
  binaries.

## Open English WordNet (OEWN) 2025

- **Location:** raw data is read read-only from the goldmines sandbox; generated
  tables live in `crates/tc-lexicon/src/generated/`.
- **License:** CC-BY-4.0
- **License text:** <https://creativecommons.org/licenses/by/4.0/legalcode>
- **Provenance:** `goldmines/PROVENANCE.md`
- **Use:** provenance-backed WordNet lemmas, synsets, and semantic concepts.

## VerbNet 3.4

- **Location:** raw data is read read-only from the goldmines sandbox; generated
  tables live in `crates/tc-lexicon/src/generated/`.
- **License:** CC-BY-4.0
- **License text:** <https://creativecommons.org/licenses/by/4.0/legalcode>
- **Provenance:** `goldmines/PROVENANCE.md`
- **Use:** provenance-backed VerbNet classes, frames, and predicates.

## Zed Extension API (`zed_extension_api`)

- **Location:** `editors/zed/tc-eng-extension/` (standalone; not a `tc_r1`
  workspace member).
- **License:** Apache-2.0
- **License text:** <https://www.apache.org/licenses/LICENSE-2.0>
- **Upstream:** <https://github.com/zed-industries/zed>
- **Use:** Zed development-extension scaffold that launches `tc-lsp`.
