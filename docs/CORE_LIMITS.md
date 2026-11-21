# Core Limits Catalog

Verified architectural boundaries. Every reproducer below returns a clean diagnostic or a deliberate COMMENT_OMITTED verdict; none panics, segfaults, or hangs.

## LIM-CORE-001 — Reserved control keyword cannot be used as a VerbNet predicate verb.

- **Layer:** LANGUAGE
- **Repro:** `battletest_core/corpora/tier_c10/limits/lim_control_keyword.tc`
- **Build exit:** 1
- **Diagnostic:** `tc build: lim_control_keyword:1:1: Unsupported: reserved control keyword `continue` takes no operands; define a Phrase: signature instead`
- **Reason:** continue is a Core-7 control anchor; a VerbNet frame using it as a verb is rejected with UNSUPPORTED.

## LIM-CORE-002 — No cross-sentence anaphora or world-knowledge inference.

- **Layer:** SCOPE
- **Repro:** `battletest_core/corpora/tier_c10/limits/lim_cross_sentence_anaphora.tc`
- **Build exit:** 0
- **Diagnostic:** `lim_cross_sentence_anaphora:1:1 COMMENT_OMITTED (past-tense or third-person narrative prose) lg=true words=5 links=7 voice=active verb=bought lemma=buy predicate=Receive concept=Transfer verbnet=accept-77.1 wordnet=oewn-02288941-v`
- **Reason:** Narrative prose is COMMENT_OMITTED; Core-7 requires explicit variables and directives.

## LIM-CORE-003 — Single-expression complexity is capped at 1024 parser charges.

- **Layer:** LANGUAGE
- **Repro:** `battletest_core/corpora/tier_c10/limits/lim_expression_budget.tc`
- **Build exit:** 1
- **Diagnostic:** `tc build: lim_expression_budget:1:1: Lowering: expression exceeds the Core-7 complexity limit`
- **Reason:** The cap prevents native stack exhaustion from pathological AST depth.

## LIM-CORE-004 — Link Grammar produces no linkage for a 300-word sentence.

- **Layer:** UPSTREAM
- **Repro:** `battletest_core/corpora/tier_c10/limits/lim_long_sentence.tc`
- **Build exit:** 0
- **Diagnostic:** `lim_long_sentence:1:1 COMMENT_OMITTED (no computational directive) lg=false words=0 links=0 voice=active`
- **Reason:** tc-syntax falls back deterministically and omits the sentence as non-computational; no panic or hang.

## LIM-CORE-005 — Threads and OS/UI/library services are outside finite Core-7 English.

- **Layer:** SCOPE
- **Repro:** `battletest_core/corpora/tier_c10/limits/lim_out_of_scope_thread.tc`
- **Build exit:** 1
- **Diagnostic:** `tc build: lim_out_of_scope_thread:1:1: Unsupported: computational verb `thread` (Assign) has no Core-7 lowering; declare a Phrase: signature or use an assignment/transfer/arithmetic form`
- **Reason:** Such directives fail loudly as UNSUPPORTED instead of being silently omitted.

## LIM-CORE-006 — Recursive aggregate values have no finite System V layout.

- **Layer:** LANGUAGE
- **Repro:** `battletest_core/corpora/tier_c10/limits/lim_recursive_struct.tc`
- **Build exit:** 1
- **Diagnostic:** `tc build: lim_recursive_struct:2:1: TypeCheck: aggregate has no finite layout (recursive type?)`
- **Reason:** The type checker rejects infinitely sized structs before layout/codegen.

## LIM-CORE-007 — Type nesting is capped at 64 levels.

- **Layer:** LANGUAGE
- **Repro:** `battletest_core/corpora/tier_c10/limits/lim_type_nesting.tc`
- **Build exit:** 1
- **Diagnostic:** `tc build: lim_type_nesting:1:1: Declaration: type nesting exceeds the Core-7 limit`
- **Reason:** The cap prevents stack exhaustion in generic/array type parsing and layout.
