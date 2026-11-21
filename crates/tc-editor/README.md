# R1 Editor

Native, `std`-only terminal IDE for `tc_r1` `.eng` English programs.

```bash
make build
./target/debug/r1-editor
./target/debug/r1-editor budget.eng
./target/debug/r1-editor --list-presets
./target/debug/r1-editor --check budget.eng
```

## Visual system

- **Themes (`F4`):** Midnight, High-Contrast, Monochrome (zero RGB escapes).
  `NO_COLOR` starts in Monochrome.
- **Semantic highlighting:** structural anchors and hardware types are
  closed-class; open-class computational verbs come only from
  `tc_lexicon::classify_verb`, never from a hardcoded list in the editor.
- **Line state:** dim `·` prose-omitted gutters and crimson `✗` diagnostic
  gutters are refreshed by `F5`/`F6`/`F7` and cleared as soon as the line is
  edited.
- **Layout:** rounded Unicode panes, focused border accents, tab strip,
  scrollbars, pills, synchronized flicker-free frames, and centered `F1`/`F2`
  modals.

## Keys

```text
Ctrl+S Save   Ctrl+O Open   Ctrl+N New    Ctrl+Q Quit
Ctrl+Z Undo   Ctrl+Y Redo   Ctrl+F Find   Ctrl+E Pane
Ctrl+Left/Right word jump
F1 Help       F2 Presets    F3 Stdin      F4 Theme
F5 / Ctrl+R   Run native ELF64 + interpreter parity
F6 / Ctrl+P   Paraphrase echo
F7 / Ctrl+T   Trace + WordNet/VerbNet provenance
F8 / Ctrl+I   Core-7 IR
F9 / Ctrl+B   Build <stem>.elf
```

## Release

```bash
CARGO_NET_OFFLINE=true cargo build --release --bin r1-editor
./target/release/r1-editor
```
