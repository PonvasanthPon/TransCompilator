//! Zero-ML, provenance-backed WordNet and VerbNet lexicon engine.

pub mod closed_class;
pub mod generated;
pub mod lemmatize;

use std::borrow::Cow;

pub use closed_class::CLOSED_CLASS;
pub use generated::morphology::{IRREGULAR_LEMMA_COUNT, IRREGULAR_LEMMAS};
pub use generated::verbnet_frames::{VERBNET_FRAME_COUNT, VERBNET_FRAMES};
pub use generated::wordnet_concepts::{WORDNET_ENTRIES, WORDNET_ENTRY_COUNT};
pub use lemmatize::lemmatize;

/// WordNet part-of-speech tag.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Pos {
    /// Verb.
    Verb,
    /// Noun.
    Noun,
    /// Adjective or adjective satellite.
    Adj,
    /// Adverb.
    Adv,
}

/// The finite computational concepts distilled from WordNet hypernyms.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SemanticConcept {
    /// Addition.
    Add,
    /// Subtraction.
    Sub,
    /// Multiplication.
    Mul,
    /// Division.
    Div,
    /// Remainder/modulo.
    Mod,
    /// Arithmetic negation.
    Negate,
    /// Doubling.
    Double,
    /// Halving.
    Halve,
    /// Assignment/storage.
    Assign,
    /// Transfer between places or owners.
    Transfer,
    /// Exchange/swap.
    Swap,
    /// Greater-than comparison.
    CompareGt,
    /// Less-than comparison.
    CompareLt,
    /// Equality comparison.
    CompareEq,
    /// Inequality comparison.
    CompareNe,
    /// Creation.
    Create,
    /// Destruction/removal.
    Destroy,
    /// Output/emission.
    Output,
    /// Input/receipt.
    Input,
    /// Iteration.
    Loop,
    /// Branch/selection.
    Branch,
    /// Return.
    Return,
}

/// Closed semantic predicates extracted from VerbNet class semantics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum VerbPredicate {
    /// Additive transfer.
    TransferAdd,
    /// Subtractive transfer.
    TransferSub,
    /// Movement/transfer between endpoints.
    TransferMove,
    /// Placement/assignment/storage.
    Assign,
    /// Creation.
    Create,
    /// Destruction/deletion.
    Destroy,
    /// Emission/communication.
    Emit,
    /// Reception/input.
    Receive,
    /// Combination/merge.
    Combine,
    /// Transformation/scaling.
    Transform,
}

/// Thematic role bound to a preposition.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PrepRole {
    /// Origin endpoint.
    Source,
    /// Goal endpoint.
    Destination,
    /// Theme/topic.
    Theme,
    /// Recipient/beneficiary.
    Recipient,
    /// Extent/measure.
    Extent,
    /// Instrument/means.
    Instrument,
    /// Location.
    Location,
}

/// English closed-class word categories.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ClosedClass {
    /// Preposition.
    Preposition,
    /// Conjunction.
    Conjunction,
    /// Determiner.
    Determiner,
    /// Auxiliary/modal.
    Auxiliary,
    /// Pronoun.
    Pronoun,
    /// Negation.
    Negation,
    /// Discourse filler.
    DiscourseFiller,
    /// Structural/control anchor.
    ControlAnchor,
}

/// One provenance-carrying WordNet row.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct WordNetEntry {
    /// Canonical lemma.
    pub lemma: &'static str,
    /// Part of speech.
    pub pos: Pos,
    /// Computational concept.
    pub concept: SemanticConcept,
    /// Exact Open English WordNet synset id.
    pub synset_id: &'static str,
    /// Hypernym distance from the concept anchor.
    pub depth: u8,
}

impl WordNetEntry {
    /// Creates a table row.
    #[must_use]
    pub const fn new(
        lemma: &'static str,
        pos: Pos,
        concept: SemanticConcept,
        synset_id: &'static str,
        depth: u8,
    ) -> Self {
        Self {
            lemma,
            pos,
            concept,
            synset_id,
            depth,
        }
    }
}

/// One provenance-carrying VerbNet frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct VerbNetFrame {
    /// Verb lemma.
    pub verb: &'static str,
    /// Preposition, or the empty string for class-level/no-prep information.
    pub prep: &'static str,
    /// Thematic role.
    pub role: PrepRole,
    /// Semantic predicate.
    pub predicate: VerbPredicate,
    /// Exact VerbNet class/subclass id.
    pub class_id: &'static str,
}

impl VerbNetFrame {
    /// Creates a frame row.
    #[must_use]
    pub const fn new(
        verb: &'static str,
        prep: &'static str,
        role: PrepRole,
        predicate: VerbPredicate,
        class_id: &'static str,
    ) -> Self {
        Self {
            verb,
            prep,
            role,
            predicate,
            class_id,
        }
    }
}

/// A resolved computational verb.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerbResolution {
    /// Lemmatized verb.
    pub lemma: String,
    /// VerbNet predicate when one was found.
    pub predicate: Option<VerbPredicate>,
    /// Preposition thematic role when a preposition was supplied.
    pub role: Option<PrepRole>,
    /// WordNet computational concept when one was found.
    pub concept: Option<SemanticConcept>,
    /// Provenance VerbNet class id.
    pub class_id: Option<&'static str>,
    /// Provenance WordNet synset id.
    pub synset_id: Option<&'static str>,
}

/// Compact lexicon table statistics.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LexiconStats {
    /// WordNet rows.
    pub wordnet_entries: usize,
    /// VerbNet frames.
    pub verbnet_frames: usize,
    /// Irregular morphology rows.
    pub irregular_lemmas: usize,
    /// Closed-class words.
    pub closed_class_words: usize,
}

/// Returns all WordNet rows for a lemma, optionally filtered by POS.
#[must_use]
pub fn lookup_wordnet(lemma: &str, pos: Option<Pos>) -> &'static [WordNetEntry] {
    let start = WORDNET_ENTRIES.partition_point(|entry| entry.lemma < lemma);
    let end = start + WORDNET_ENTRIES[start..].partition_point(|entry| entry.lemma == lemma);
    let lemma_entries = &WORDNET_ENTRIES[start..end];
    let Some(pos) = pos else {
        return lemma_entries;
    };
    let pos_start = lemma_entries.partition_point(|entry| entry.pos < pos);
    let pos_end = pos_start + lemma_entries[pos_start..].partition_point(|entry| entry.pos == pos);
    &lemma_entries[pos_start..pos_end]
}

/// Returns all VerbNet frames for a verb, optionally filtered by preposition.
#[must_use]
pub fn lookup_verbnet(verb: &str, prep: Option<&str>) -> &'static [VerbNetFrame] {
    if let Some(prep) = prep {
        let start = VERBNET_FRAMES.partition_point(|frame| {
            frame.verb < verb || (frame.verb == verb && frame.prep < prep)
        });
        let count = VERBNET_FRAMES[start..]
            .partition_point(|frame| frame.verb == verb && frame.prep == prep);
        return &VERBNET_FRAMES[start..start + count];
    }
    let start = VERBNET_FRAMES.partition_point(|frame| frame.verb < verb);
    let count = VERBNET_FRAMES[start..].partition_point(|frame| frame.verb == verb);
    &VERBNET_FRAMES[start..start + count]
}

/// Looks up a closed-class word.
#[must_use]
pub fn lookup_closed_class(word: &str) -> Option<ClosedClass> {
    CLOSED_CLASS
        .binary_search_by(|(candidate, _)| (*candidate).cmp(word))
        .ok()
        .map(|index| CLOSED_CLASS[index].1)
}

/// Maps a preposition to its thematic role.
#[must_use]
pub fn prep_role(prep: &str) -> Option<PrepRole> {
    match prep.to_lowercase().as_str() {
        "from" | "out_of" | "off" => Some(PrepRole::Source),
        "to" | "into" | "in" | "onto" => Some(PrepRole::Destination),
        "of" | "about" => Some(PrepRole::Theme),
        "for" => Some(PrepRole::Recipient),
        "by" => Some(PrepRole::Extent),
        "with" | "using" => Some(PrepRole::Instrument),
        "at" | "on" | "inside" => Some(PrepRole::Location),
        _ => None,
    }
}

fn concept_priority(concept: SemanticConcept) -> u8 {
    match concept {
        SemanticConcept::Add => 0,
        SemanticConcept::Sub => 1,
        SemanticConcept::Mul => 2,
        SemanticConcept::Div => 3,
        SemanticConcept::Mod => 4,
        SemanticConcept::Negate => 5,
        SemanticConcept::Double => 6,
        SemanticConcept::Halve => 7,
        SemanticConcept::Assign => 8,
        SemanticConcept::Transfer => 9,
        SemanticConcept::Swap => 10,
        SemanticConcept::CompareGt => 11,
        SemanticConcept::CompareLt => 12,
        SemanticConcept::CompareEq => 13,
        SemanticConcept::CompareNe => 14,
        SemanticConcept::Create => 15,
        SemanticConcept::Destroy => 16,
        SemanticConcept::Output => 17,
        SemanticConcept::Input => 18,
        SemanticConcept::Loop => 19,
        SemanticConcept::Return => 20,
        SemanticConcept::Branch => 21,
    }
}

fn concept_predicate(concept: SemanticConcept) -> Option<VerbPredicate> {
    match concept {
        SemanticConcept::Add => Some(VerbPredicate::TransferAdd),
        SemanticConcept::Sub => Some(VerbPredicate::TransferSub),
        SemanticConcept::Assign => Some(VerbPredicate::Assign),
        SemanticConcept::Transfer => Some(VerbPredicate::TransferMove),
        SemanticConcept::Create => Some(VerbPredicate::Create),
        SemanticConcept::Destroy => Some(VerbPredicate::Destroy),
        SemanticConcept::Output => Some(VerbPredicate::Emit),
        SemanticConcept::Input => Some(VerbPredicate::Receive),
        _ => None,
    }
}

fn frame_priority(predicate: VerbPredicate, prep: Option<&str>) -> u8 {
    match prep {
        Some("from") => match predicate {
            VerbPredicate::TransferSub => 0,
            VerbPredicate::Receive => 1,
            VerbPredicate::TransferMove => 2,
            VerbPredicate::Transform => 3,
            VerbPredicate::Combine => 4,
            VerbPredicate::Assign => 5,
            VerbPredicate::Emit => 6,
            VerbPredicate::Create => 7,
            VerbPredicate::Destroy => 8,
            VerbPredicate::TransferAdd => 9,
        },
        Some("to" | "into" | "in" | "onto") => match predicate {
            VerbPredicate::TransferAdd => 0,
            VerbPredicate::Combine => 1,
            VerbPredicate::TransferMove => 2,
            VerbPredicate::Assign => 3,
            VerbPredicate::Emit => 4,
            VerbPredicate::Receive => 5,
            VerbPredicate::Transform => 6,
            VerbPredicate::Create => 7,
            VerbPredicate::Destroy => 8,
            VerbPredicate::TransferSub => 9,
        },
        Some("by") => match predicate {
            VerbPredicate::Transform => 0,
            VerbPredicate::Assign => 1,
            VerbPredicate::TransferMove => 2,
            _ => 3,
        },
        Some("with") => match predicate {
            VerbPredicate::Combine => 0,
            VerbPredicate::Transform => 1,
            VerbPredicate::Assign => 2,
            VerbPredicate::Emit => 3,
            _ => 4,
        },
        _ => match predicate {
            VerbPredicate::Assign => 0,
            VerbPredicate::Create => 1,
            VerbPredicate::Destroy => 2,
            VerbPredicate::TransferAdd => 3,
            VerbPredicate::Combine => 4,
            VerbPredicate::TransferSub => 5,
            VerbPredicate::Receive => 6,
            VerbPredicate::Emit => 7,
            VerbPredicate::TransferMove => 8,
            VerbPredicate::Transform => 9,
        },
    }
}

fn best_frame(
    frames: &'static [VerbNetFrame],
    prep: Option<&str>,
) -> Option<&'static VerbNetFrame> {
    frames
        .iter()
        .min_by_key(|frame| (frame_priority(frame.predicate, prep), frame.class_id))
}

fn best_wordnet_verb(lemma: &str) -> Option<&'static WordNetEntry> {
    lookup_wordnet(lemma, Some(Pos::Verb))
        .iter()
        .min_by_key(|entry| (concept_priority(entry.concept), entry.synset_id))
}

/// Resolves a raw English verb, including inflection, to a closed semantic
/// predicate/concept with provenance.  Non-computational prose verbs return
/// `None`.
#[must_use]
pub fn classify_verb(raw_word: &str, prep: Option<&str>) -> Option<VerbResolution> {
    let lemma_cow: Cow<'_, str> = lemmatize(raw_word);
    let lemma = lemma_cow.as_ref();
    let normalized_prep = prep.map(|value| value.trim().to_lowercase());
    let prep_slice = normalized_prep.as_deref();

    let some_frame = if let Some(prep) = prep_slice {
        let exact = lookup_verbnet(lemma, Some(prep));
        if exact.is_empty() {
            let general = lookup_verbnet(lemma, Some(""));
            if general.is_empty() {
                best_frame(lookup_verbnet(lemma, None), Some(prep))
            } else {
                best_frame(general, Some(prep))
            }
        } else {
            best_frame(exact, Some(prep))
        }
    } else {
        let general = lookup_verbnet(lemma, Some(""));
        if general.is_empty() {
            best_frame(lookup_verbnet(lemma, None), None)
        } else {
            best_frame(general, None)
        }
    };

    let wordnet = best_wordnet_verb(lemma);
    if some_frame.is_none() && wordnet.is_none() {
        return None;
    }

    let predicate = some_frame
        .map(|frame| frame.predicate)
        .or_else(|| wordnet.and_then(|entry| concept_predicate(entry.concept)));
    let role = match (prep_slice, some_frame) {
        (Some(_prep), Some(frame)) if !frame.prep.is_empty() => Some(frame.role),
        (Some(prep), _) => prep_role(prep),
        (None, _) => None,
    };

    Some(VerbResolution {
        lemma: lemma.to_string(),
        predicate,
        role,
        concept: wordnet.map(|entry| entry.concept),
        class_id: some_frame.map(|frame| frame.class_id),
        synset_id: wordnet.map(|entry| entry.synset_id),
    })
}

/// Returns generated lexicon table statistics.
#[must_use]
pub fn lexicon_stats() -> LexiconStats {
    LexiconStats {
        wordnet_entries: WORDNET_ENTRY_COUNT,
        verbnet_frames: VERBNET_FRAME_COUNT,
        irregular_lemmas: IRREGULAR_LEMMA_COUNT,
        closed_class_words: CLOSED_CLASS.len(),
    }
}

/// Returns the canonical package name of this crate.
#[must_use]
pub fn crate_name() -> &'static str {
    env!("CARGO_PKG_NAME")
}

#[cfg(test)]
mod tests {
    use super::{crate_name, lexicon_stats};

    #[test]
    fn crate_name_matches_package() {
        assert_eq!(crate_name(), "tc-lexicon");
    }

    #[test]
    fn crate_identity_matches_workspace() {
        assert_eq!(crate_name(), "tc-lexicon");
        let stats = lexicon_stats();
        assert!(stats.wordnet_entries > 0);
        assert!(stats.verbnet_frames > 0);
        assert!(stats.irregular_lemmas > 0);
        assert!(stats.closed_class_words > 0);
    }
}
