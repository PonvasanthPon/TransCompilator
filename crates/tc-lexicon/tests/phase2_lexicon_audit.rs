//! Phase 2 lexicon table integrity, morphology, resolution, and prose safety.

use tc_lexicon::{
    CLOSED_CLASS, ClosedClass, IRREGULAR_LEMMAS, Pos, PrepRole, SemanticConcept, VERBNET_FRAMES,
    VerbPredicate, WORDNET_ENTRIES, classify_verb, lemmatize, lexicon_stats, lookup_closed_class,
    lookup_verbnet, lookup_wordnet, prep_role,
};

fn resolution(word: &str, prep: Option<&str>) -> tc_lexicon::VerbResolution {
    classify_verb(word, prep).unwrap_or_else(|| panic!("expected resolution for {word}/{prep:?}"))
}

fn predicate(word: &str, prep: Option<&str>) -> VerbPredicate {
    resolution(word, prep)
        .predicate
        .unwrap_or_else(|| panic!("expected predicate for {word}/{prep:?}"))
}

fn concept(word: &str) -> SemanticConcept {
    resolution(word, None)
        .concept
        .unwrap_or_else(|| panic!("expected WordNet concept for {word}"))
}

fn assert_none_for_all(word: &str) {
    for prep in [None, Some("from"), Some("to"), Some("with")] {
        assert!(
            classify_verb(word, prep).is_none(),
            "prose verb `{word}` unexpectedly resolved with {prep:?}"
        );
    }
}

#[test]
fn wordnet_table_strictly_sorted_and_nonempty() {
    assert!(
        WORDNET_ENTRIES.len() > 334,
        "expected broader coverage than the 334-word prototype, found {}",
        WORDNET_ENTRIES.len()
    );
    for pair in WORDNET_ENTRIES.windows(2) {
        let left = (pair[0].lemma, pair[0].pos, pair[0].concept);
        let right = (pair[1].lemma, pair[1].pos, pair[1].concept);
        assert!(
            left <= right,
            "WordNet rows not sorted: {left:?} > {right:?}"
        );
    }
}

#[test]
fn wordnet_provenance_and_depth() {
    for entry in WORDNET_ENTRIES {
        assert!(entry.synset_id.starts_with("oewn-"), "{entry:?}");
        assert!(entry.depth <= 3, "{entry:?}");
        assert!(!entry.lemma.is_empty(), "{entry:?}");
    }
}

#[test]
fn wordnet_covers_every_semantic_concept() {
    for concept in [
        SemanticConcept::Add,
        SemanticConcept::Sub,
        SemanticConcept::Mul,
        SemanticConcept::Div,
        SemanticConcept::Mod,
        SemanticConcept::Negate,
        SemanticConcept::Double,
        SemanticConcept::Halve,
        SemanticConcept::Assign,
        SemanticConcept::Transfer,
        SemanticConcept::Swap,
        SemanticConcept::CompareGt,
        SemanticConcept::CompareLt,
        SemanticConcept::CompareEq,
        SemanticConcept::CompareNe,
        SemanticConcept::Create,
        SemanticConcept::Destroy,
        SemanticConcept::Output,
        SemanticConcept::Input,
        SemanticConcept::Loop,
        SemanticConcept::Branch,
        SemanticConcept::Return,
    ] {
        assert!(
            WORDNET_ENTRIES.iter().any(|entry| entry.concept == concept),
            "missing concept {concept:?}"
        );
    }
}

#[test]
fn verbnet_table_strictly_sorted_and_nonempty() {
    assert!(
        VERBNET_FRAMES.len() > 2000,
        "expected thousands of VerbNet frames, found {}",
        VERBNET_FRAMES.len()
    );
    for pair in VERBNET_FRAMES.windows(2) {
        let left = (
            pair[0].verb,
            pair[0].prep,
            pair[0].predicate,
            pair[0].class_id,
        );
        let right = (
            pair[1].verb,
            pair[1].prep,
            pair[1].predicate,
            pair[1].class_id,
        );
        assert!(
            left <= right,
            "VerbNet frames not sorted: {left:?} > {right:?}"
        );
    }
}

#[test]
fn verbnet_provenance_and_predicate_coverage() {
    for frame in VERBNET_FRAMES {
        assert!(!frame.verb.is_empty(), "{frame:?}");
        assert!(!frame.class_id.is_empty(), "{frame:?}");
        assert_ne!(frame.class_id, "unknown", "{frame:?}");
    }
    for predicate in [
        VerbPredicate::TransferAdd,
        VerbPredicate::TransferSub,
        VerbPredicate::TransferMove,
        VerbPredicate::Assign,
        VerbPredicate::Create,
        VerbPredicate::Destroy,
        VerbPredicate::Emit,
        VerbPredicate::Receive,
        VerbPredicate::Combine,
        VerbPredicate::Transform,
    ] {
        assert!(
            VERBNET_FRAMES
                .iter()
                .any(|frame| frame.predicate == predicate),
            "missing predicate {predicate:?}"
        );
    }
}

#[test]
fn morphology_table_strictly_sorted() {
    assert!(IRREGULAR_LEMMAS.len() >= 100);
    for pair in IRREGULAR_LEMMAS.windows(2) {
        assert!(
            pair[0].0 < pair[1].0,
            "irregular rows not strictly sorted: {:?} >= {:?}",
            pair[0].0,
            pair[1].0
        );
    }
    for (form, lemma) in IRREGULAR_LEMMAS {
        assert!(!form.is_empty());
        assert!(!lemma.is_empty());
    }
}

#[test]
fn closed_class_table_strictly_sorted() {
    assert!(CLOSED_CLASS.len() >= 200);
    for pair in CLOSED_CLASS.windows(2) {
        assert!(
            pair[0].0 < pair[1].0,
            "closed-class rows not sorted: {:?} >= {:?}",
            pair[0].0,
            pair[1].0
        );
    }
    for category in [
        ClosedClass::Preposition,
        ClosedClass::Conjunction,
        ClosedClass::Determiner,
        ClosedClass::Auxiliary,
        ClosedClass::Pronoun,
        ClosedClass::Negation,
        ClosedClass::DiscourseFiller,
        ClosedClass::ControlAnchor,
    ] {
        assert!(
            CLOSED_CLASS.iter().any(|(_, found)| *found == category),
            "missing closed-class category {category:?}"
        );
    }
}

#[test]
fn lexicon_stats_are_consistent() {
    let stats = lexicon_stats();
    assert_eq!(stats.wordnet_entries, WORDNET_ENTRIES.len());
    assert_eq!(stats.verbnet_frames, VERBNET_FRAMES.len());
    assert_eq!(stats.irregular_lemmas, IRREGULAR_LEMMAS.len());
    assert_eq!(stats.closed_class_words, CLOSED_CLASS.len());
    assert!(stats.wordnet_entries > 334);
    assert!(stats.verbnet_frames > 2000);
}

#[test]
fn irregular_lemmatizer_accuracy() {
    for (word, lemma) in [
        ("spent", "spend"),
        ("took", "take"),
        ("taken", "take"),
        ("gave", "give"),
        ("given", "give"),
        ("became", "become"),
        ("told", "tell"),
        ("said", "say"),
        ("bought", "buy"),
        ("wrote", "write"),
        ("written", "write"),
        ("built", "build"),
        ("ran", "run"),
        ("began", "begin"),
        ("begun", "begin"),
        ("drew", "draw"),
        ("drawn", "draw"),
        ("grew", "grow"),
        ("grown", "grow"),
        ("knew", "know"),
        ("known", "know"),
        ("spoke", "speak"),
        ("spoken", "speak"),
        ("swept", "sweep"),
    ] {
        assert_eq!(lemmatize(word).as_ref(), lemma, "lemma {word}");
    }
}

#[test]
fn regular_ed_and_ing_lemmatizer() {
    for (word, lemma) in [
        ("removed", "remove"),
        ("added", "add"),
        ("multiplied", "multiply"),
        ("dividing", "divide"),
        ("deducted", "deduct"),
        ("increases", "increase"),
        ("dropped", "drop"),
        ("stopped", "stop"),
        ("running", "run"),
        ("baked", "bake"),
        ("cooked", "cook"),
    ] {
        assert_eq!(lemmatize(word).as_ref(), lemma, "lemma {word}");
    }
}

#[test]
fn regular_s_and_ies_lemmatizer() {
    assert_eq!(lemmatize("multiplies").as_ref(), "multiply");
    assert_eq!(lemmatize("spends").as_ref(), "spend");
    assert_eq!(lemmatize("adds").as_ref(), "add");
    assert_eq!(lemmatize("accepts").as_ref(), "accept");
    assert_eq!(lemmatize("creates").as_ref(), "create");
    assert_eq!(lemmatize("divides").as_ref(), "divide");
}

#[test]
fn protected_words_are_stable() {
    for word in [
        "less", "plus", "times", "minus", "bus", "gas", "class", "exists", "equals", "series",
    ] {
        assert_eq!(lemmatize(word).as_ref(), word, "protected {word}");
    }
}

#[test]
fn uppercase_and_identity_lemmatizer() {
    assert_eq!(lemmatize("REMOVED").as_ref(), "remove");
    assert_eq!(lemmatize("SPENT").as_ref(), "spend");
    for word in ["put", "cut", "set", "read", "split"] {
        assert_eq!(lemmatize(word).as_ref(), word);
    }
}

#[test]
fn subtractive_verbs_bind_source() {
    for word in ["remove", "deduct", "withdraw", "subtract", "take", "spend"] {
        let resolved = resolution(word, Some("from"));
        assert_eq!(
            resolved.predicate,
            Some(VerbPredicate::TransferSub),
            "{word}"
        );
        assert_eq!(resolved.role, Some(PrepRole::Source), "{word}");
        assert!(resolved.class_id.is_some(), "{word}");
    }
}

#[test]
fn subtractive_inflections_bind_source() {
    for (word, lemma) in [("removed", "remove"), ("deducted", "deduct")] {
        let resolved = resolution(word, Some("from"));
        assert_eq!(resolved.lemma, lemma);
        assert_eq!(resolved.predicate, Some(VerbPredicate::TransferSub));
        assert_eq!(resolved.role, Some(PrepRole::Source));
    }
}

#[test]
fn subtractive_verbnet_provenance() {
    let frames = lookup_verbnet("remove", Some("from"));
    assert!(
        frames.iter().any(|frame| {
            frame.class_id == "remove-10.1"
                && frame.predicate == VerbPredicate::TransferSub
                && frame.role == PrepRole::Source
        }),
        "remove/from must include remove-10.1 Source TransferSub"
    );
    for word in ["remove", "deduct", "withdraw", "subtract", "take", "spend"] {
        assert!(
            lookup_verbnet(word, Some("from")).iter().any(|frame| {
                frame.predicate == VerbPredicate::TransferSub && frame.role == PrepRole::Source
            }),
            "{word}/from lacks TransferSub+Source"
        );
    }
}

#[test]
fn additive_verbs_bind_destination() {
    for (word, prep) in [
        ("add", "to"),
        ("deposit", "to"),
        ("combine", "into"),
        ("append", "to"),
    ] {
        let resolved = resolution(word, Some(prep));
        assert!(
            matches!(
                resolved.predicate,
                Some(VerbPredicate::TransferAdd | VerbPredicate::Combine)
            ),
            "{word}/{prep}: {:?}",
            resolved.predicate
        );
        assert_eq!(resolved.role, Some(PrepRole::Destination), "{word}");
    }
}

#[test]
fn additive_inflected_form() {
    let resolved = resolution("added", Some("to"));
    assert_eq!(resolved.lemma, "add");
    assert_eq!(resolved.predicate, Some(VerbPredicate::TransferAdd));
    assert_eq!(resolved.role, Some(PrepRole::Destination));
}

#[test]
fn assignment_verbs_resolve() {
    for word in ["put", "store", "place", "keep", "set", "assign"] {
        assert_eq!(
            predicate(word, None),
            VerbPredicate::Assign,
            "assignment verb {word}"
        );
    }
}

#[test]
fn output_verbs_resolve() {
    for word in ["say", "tell", "show", "display", "print"] {
        assert_eq!(
            predicate(word, None),
            VerbPredicate::Emit,
            "output verb {word}"
        );
    }
    let output = resolution("output", None);
    assert_eq!(output.predicate, Some(VerbPredicate::Emit));
    assert_eq!(output.concept, Some(SemanticConcept::Output));
    assert!(output.synset_id.is_some());
}

#[test]
fn creation_verbs_resolve() {
    for word in ["create", "build"] {
        assert_eq!(
            predicate(word, None),
            VerbPredicate::Create,
            "creation verb {word}"
        );
    }
}

#[test]
fn destruction_verbs_resolve() {
    for word in ["delete", "destroy", "erase"] {
        assert_eq!(
            predicate(word, None),
            VerbPredicate::Destroy,
            "destruction verb {word}"
        );
    }
}

#[test]
fn arithmetic_scaling_concepts() {
    let multiply = resolution("multiply", Some("by"));
    assert_eq!(multiply.concept, Some(SemanticConcept::Mul));
    assert_eq!(multiply.role, Some(PrepRole::Extent));
    assert_eq!(concept("double"), SemanticConcept::Double);
    assert_eq!(concept("divide"), SemanticConcept::Div);
    assert_eq!(concept("halve"), SemanticConcept::Halve);
    assert_eq!(concept("subtract"), SemanticConcept::Sub);
    assert_eq!(concept("add"), SemanticConcept::Add);
}

#[test]
fn arithmetic_inflected_forms() {
    let resolved = resolution("multiplied", Some("by"));
    assert_eq!(resolved.lemma, "multiply");
    assert_eq!(resolved.concept, Some(SemanticConcept::Mul));
    assert_eq!(resolved.role, Some(PrepRole::Extent));
    assert_eq!(concept("doubled"), SemanticConcept::Double);
    assert_eq!(concept("divided"), SemanticConcept::Div);
}

#[test]
fn receive_verbs_resolve() {
    for word in ["get", "obtain", "receive", "read", "accept"] {
        assert_eq!(
            predicate(word, None),
            VerbPredicate::Receive,
            "receive verb {word}"
        );
    }
}

#[test]
fn prep_role_mapping() {
    assert_eq!(prep_role("from"), Some(PrepRole::Source));
    assert_eq!(prep_role("to"), Some(PrepRole::Destination));
    assert_eq!(prep_role("into"), Some(PrepRole::Destination));
    assert_eq!(prep_role("by"), Some(PrepRole::Extent));
    assert_eq!(prep_role("with"), Some(PrepRole::Instrument));
    assert_eq!(prep_role("of"), Some(PrepRole::Theme));
    assert_eq!(prep_role("for"), Some(PrepRole::Recipient));
    assert_eq!(prep_role("at"), Some(PrepRole::Location));
    assert_eq!(prep_role("underneath-nonsense"), None);
}

#[test]
fn lookup_verbnet_prep_filtering() {
    let from_frames = lookup_verbnet("remove", Some("from"));
    assert!(!from_frames.is_empty());
    assert!(from_frames.iter().all(|frame| frame.prep == "from"));
    let all_frames = lookup_verbnet("remove", None);
    assert!(all_frames.len() >= from_frames.len());
    assert!(all_frames.iter().all(|frame| frame.verb == "remove"));
}

#[test]
fn lookup_wordnet_pos_filtering() {
    let verbs = lookup_wordnet("multiply", Some(Pos::Verb));
    assert!(!verbs.is_empty());
    assert!(verbs.iter().all(|entry| entry.pos == Pos::Verb));
    assert!(
        lookup_wordnet("multiply", Some(Pos::Noun))
            .iter()
            .all(|entry| entry.pos == Pos::Noun)
    );
    let less = lookup_wordnet("less", Some(Pos::Adj));
    assert!(
        less.iter()
            .any(|entry| entry.concept == SemanticConcept::CompareLt)
    );
}

#[test]
fn unknown_words_return_none() {
    assert!(classify_verb("qwertyuiop", None).is_none());
    assert!(classify_verb("flibbertigibbet", Some("from")).is_none());
}

#[test]
fn closed_class_prepositions() {
    for word in ["from", "to", "into", "by", "of"] {
        assert_eq!(
            lookup_closed_class(word),
            Some(ClosedClass::Preposition),
            "{word}"
        );
    }
}

#[test]
fn closed_class_control_anchors() {
    for word in ["if", "else", "loop", "function", "struct", "syscall"] {
        assert_eq!(
            lookup_closed_class(word),
            Some(ClosedClass::ControlAnchor),
            "{word}"
        );
    }
}

#[test]
fn closed_class_discourse_fillers() {
    for word in ["okay", "please", "first", "finally"] {
        assert_eq!(
            lookup_closed_class(word),
            Some(ClosedClass::DiscourseFiller),
            "{word}"
        );
    }
}

#[test]
fn closed_class_pronouns() {
    for word in ["he", "she", "it", "they"] {
        assert_eq!(
            lookup_closed_class(word),
            Some(ClosedClass::Pronoun),
            "{word}"
        );
    }
}

#[test]
fn pos_store_noun_not_assign() {
    assert!(
        lookup_wordnet("store", Some(Pos::Noun))
            .iter()
            .all(|entry| entry.concept != SemanticConcept::Assign)
    );
}

#[test]
fn pos_very_adj_not_compare_eq() {
    assert!(
        lookup_wordnet("very", Some(Pos::Adj))
            .iter()
            .all(|entry| entry.concept != SemanticConcept::CompareEq)
    );
}

#[test]
fn pos_last_adj_not_compare_lt() {
    assert!(
        lookup_wordnet("last", Some(Pos::Adj))
            .iter()
            .all(|entry| entry.concept != SemanticConcept::CompareLt)
    );
}

#[test]
fn prose_motion_verbs_none() {
    for word in [
        "walk",
        "walked",
        "travel",
        "traveled",
        "travelled",
        "swim",
        "swam",
    ] {
        assert_none_for_all(word);
    }
}

#[test]
fn prose_weather_and_sleep_none() {
    for word in ["rain", "rained", "sleep", "slept"] {
        assert_none_for_all(word);
    }
}

#[test]
fn prose_consumption_none() {
    for word in ["drink", "drank", "eat", "ate"] {
        assert_none_for_all(word);
    }
}

#[test]
fn prose_waiting_and_love_none() {
    for word in ["wait", "waited", "love", "loved"] {
        assert_none_for_all(word);
    }
}

#[test]
fn prose_laughter_and_smile_none() {
    for word in ["laugh", "laughed", "smile", "smiled"] {
        assert_none_for_all(word);
    }
}

#[test]
fn prose_cooking_none() {
    for word in ["bake", "baked", "cook", "cooked"] {
        assert_none_for_all(word);
    }
}

#[test]
fn prose_light_verbs_none() {
    for word in ["be", "have", "do", "go", "come", "run", "watch"] {
        assert_none_for_all(word);
    }
}

#[test]
fn wordnet_arithmetic_entries() {
    assert!(
        lookup_wordnet("subtract", Some(Pos::Verb))
            .iter()
            .any(|entry| entry.concept == SemanticConcept::Sub)
    );
    assert!(
        lookup_wordnet("add", Some(Pos::Verb))
            .iter()
            .any(|entry| entry.concept == SemanticConcept::Add)
    );
    assert!(
        lookup_wordnet("multiply", Some(Pos::Verb))
            .iter()
            .any(|entry| entry.concept == SemanticConcept::Mul)
    );
    assert!(
        lookup_wordnet("divide", Some(Pos::Verb))
            .iter()
            .any(|entry| entry.concept == SemanticConcept::Div)
    );
    assert!(
        lookup_wordnet("halve", Some(Pos::Verb))
            .iter()
            .any(|entry| entry.concept == SemanticConcept::Halve)
    );
    assert!(
        lookup_wordnet("remainder", Some(Pos::Noun))
            .iter()
            .any(|entry| entry.concept == SemanticConcept::Mod)
    );
}

#[test]
fn wordnet_transfer_and_control_entries() {
    assert!(
        lookup_wordnet("transfer", Some(Pos::Verb))
            .iter()
            .any(|entry| entry.concept == SemanticConcept::Transfer)
    );
    assert!(
        lookup_wordnet("swap", Some(Pos::Verb))
            .iter()
            .any(|entry| entry.concept == SemanticConcept::Swap)
    );
    assert!(
        lookup_wordnet("create", Some(Pos::Verb))
            .iter()
            .any(|entry| entry.concept == SemanticConcept::Create)
    );
    assert!(
        lookup_wordnet("destroy", Some(Pos::Verb))
            .iter()
            .any(|entry| entry.concept == SemanticConcept::Destroy)
    );
    assert!(
        lookup_wordnet("output", Some(Pos::Verb))
            .iter()
            .any(|entry| entry.concept == SemanticConcept::Output)
    );
    assert!(
        lookup_wordnet("repeat", Some(Pos::Verb))
            .iter()
            .any(|entry| entry.concept == SemanticConcept::Loop)
    );
    assert!(
        lookup_wordnet("choose", Some(Pos::Verb))
            .iter()
            .any(|entry| entry.concept == SemanticConcept::Branch)
    );
    assert!(
        lookup_wordnet("return", Some(Pos::Verb))
            .iter()
            .any(|entry| entry.concept == SemanticConcept::Return)
    );
}

#[test]
fn verbnet_subtractive_class_bindings() {
    for class_id in ["remove-10.1", "banish-10.2", "steal-10.5"] {
        assert!(
            VERBNET_FRAMES.iter().any(|frame| {
                frame.class_id == class_id
                    && frame.prep == "from"
                    && frame.predicate == VerbPredicate::TransferSub
                    && frame.role == PrepRole::Source
            }),
            "{class_id} must bind from -> Source + TransferSub"
        );
    }
}

#[test]
fn known_verb_lemma_table_is_sorted_and_nonempty() {
    use tc_lexicon::generated::morphology::VERB_LEMMAS;

    assert!(VERB_LEMMAS.len() > 1000);
    for pair in VERB_LEMMAS.windows(2) {
        assert!(pair[0] < pair[1], "verb lemmas not sorted: {pair:?}");
    }
}
