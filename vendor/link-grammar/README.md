# Vendored CMU Link Grammar

This directory contains an unmodified runtime copy of CMU Link Grammar
5.13.2, used by the Rust front end for English parsing.

- **Upstream:** <https://github.com/opencog/link-grammar>
- **Version:** 5.13.2
- **License:** LGPL-2.1-or-later; see `LICENSE` in this directory.
- **Contents:** public headers, the shared library and its version symlinks,
  and the English dictionary/word files under `share/link-grammar/`.
- **Modifications:** none. The sources are not bundled; the unmodified shared
  library is redistributed for local builds and tests.

The `$ORIGIN`-relative run paths emitted by the crate build scripts keep the
shared library discoverable from `target/debug` and `target/release` without a
system-wide install.
