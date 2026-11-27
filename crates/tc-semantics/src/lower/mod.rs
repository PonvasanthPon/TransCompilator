//! Declaration parsing and the single unified Core-7 statement lowerer.

#![allow(clippy::redundant_closure_for_method_calls)]

use std::collections::{BTreeMap, HashMap, HashSet};

use tc_lexicon::{SemanticConcept, VerbPredicate, classify_verb, prep_role};
use tc_typeck::{
    AstExpr, AstFn, AstGlobal, AstModule, AstStmt, AstStruct, AstType, AstUnion, Import,
    PhrasePart, PhrasePattern, Visibility,
};

use crate::clause_graph::{self, ClauseGraph};
use crate::cond::{
    CondAst, LoweredCond, parse_condition, render_cond_child_text, render_expr,
    split_header_condition, strip_outer_parentheses,
};
use crate::discourse::{
    DiscourseStack, ReferentKind, ResolvedAnaphor, ResolvedFieldAnaphor, ScopeKind,
};
use crate::discourse_scope::{
    CheckDomain, CheckKind, LargestTracker, SequenceGeneration, SequenceIteration,
    SequenceRecurrence, SequenceSeed, continuation_rest, parse_adjacent_pass, parse_check,
    parse_largest_tracker, parse_loop_filter, parse_loop_introduction, parse_move_guard,
    parse_named_list_start, parse_pass_cleanup, parse_sequence_generation,
    parse_sequence_iteration, parse_sequence_seed, parse_state_declaration, parse_stop_condition,
    post_loop_rest,
};
use crate::expr::{
    AggregateInfo, ExprEnv, parse_address, parse_expression, parse_store_address, parse_type_text,
};
use crate::intent::{IntentVerdict, classify};
use crate::paraphrase::{ParaphraseDoc, ParaphraseEntry, canonicalize_statement};
use crate::phrases::{PhraseBinding, PhraseMatch, match_all};
use crate::place::{canonical_assignment_target, parse_place};
use crate::quantifier::{
    GlobalQuantifier, LocalQuantKind, LocalQuantSpec, PredSpec, QuantDomainSpec,
    parse_global as parse_global_quantifier,
};
use crate::suggest::hint_for;
use crate::textutil::{
    find_top_level_symbol, find_top_level_word, first_word, is_lvalue_text, split_top_level,
    split_type_list, split_where, starts_with_word,
};
use crate::token::{Token, TokenKind, decode_string_token, tokenize};
use crate::value::ValueContext;
use crate::{
    ClauseTree, RawBlock, RawItem, RawSentence, SemanticError, SemanticErrorKind, SentenceTrace,
    SourceFile, Span, Voice, analyze_clause, segment_source,
};

mod control;
mod decls;
mod helpers;
mod sequences;
mod stmts;

#[cfg(test)]
mod tests;

pub(crate) use decls::*;
pub(crate) use helpers::*;

const MAX_DEPTH: usize = 32;

type AggregateHeader = (String, Vec<String>, Vec<(String, AstType)>);

/// One function whose body still has to be lowered.
#[derive(Debug)]
pub(crate) struct FunctionJob {
    /// Index in the module function table.
    pub index: usize,
    /// Body items in source order.
    pub body: Vec<RawItem>,
}

/// Result of the declaration pass for one module.
#[derive(Debug)]
pub(crate) struct ModuleJob {
    /// Partially built AST module (function bodies empty).
    pub module: AstModule,
    /// Bodies awaiting lowering.
    pub functions: Vec<FunctionJob>,
    /// Top-level executable items.
    pub executable: Vec<RawItem>,
}

fn semantic_error(
    module: &str,
    span: Span,
    kind: SemanticErrorKind,
    message: impl Into<String>,
) -> SemanticError {
    SemanticError {
        module: module.to_string(),
        line: span.line,
        col: span.col,
        kind,
        message: message.into(),
    }
}

fn type_span(module: &str, span: Span) -> tc_typeck::Span {
    tc_typeck::Span::new(module, span.line, span.col)
}

#[derive(Debug, Clone)]
struct Scope {
    vars: HashMap<String, AstType>,
}

impl Scope {
    fn new() -> Self {
        Self {
            vars: HashMap::new(),
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) enum Target {
    Local(String),
    Global(String),
    Memory(AstExpr),
}

impl Target {
    pub(crate) fn read(&self) -> AstExpr {
        match self {
            Self::Local(name) => AstExpr::Var(name.clone()),
            Self::Global(name) => AstExpr::Load(Box::new(AstExpr::GlobalAddr {
                module: None,
                name: name.clone(),
            })),
            Self::Memory(address) => AstExpr::Load(Box::new(address.clone())),
        }
    }
}

#[derive(Debug, Clone)]
struct CheckRecord {
    matched_var: String,
    kind: CheckKind,
}

#[derive(Debug, Clone)]
struct ActiveTracker {
    name: String,
    maximum: bool,
}

#[derive(Debug, Clone)]
struct AdjacentContext {
    list: String,
    index: String,
    left: String,
    right: String,
    element: AstType,
}

pub(crate) struct Lowerer<'a> {
    parser: &'a crate::LinkGrammarParser,
    module: String,
    globals: HashMap<String, AstType>,
    types: HashSet<String>,
    aggregates: HashMap<String, AggregateInfo>,
    module_names: Vec<String>,
    phrases: Vec<PhraseBinding>,
    function_returns: HashMap<String, Option<AstType>>,
    scopes: Vec<Scope>,
    discourse: DiscourseStack,
    paraphrase_entries: Vec<ParaphraseEntry>,
    active_entry: Option<usize>,
    synthetic_depth: usize,
    trace: Vec<SentenceTrace>,
    errors: Vec<SemanticError>,
    temp_counter: usize,
    loop_depth: usize,
    bytes_lengths: HashMap<String, usize>,
    active_loop_var: Option<String>,
    last_check: Option<CheckRecord>,
    implicit_counter: Option<String>,
    largest_tracker: Option<ActiveTracker>,
    print_tracker: Option<ActiveTracker>,
    running_total: Option<String>,
    named_counters: HashMap<String, String>,
    move_floor: Option<String>,
    last_iteration_var: Option<String>,
    last_list: Option<String>,
    adjacent: Option<AdjacentContext>,
}

impl<'a> Lowerer<'a> {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn new(
        parser: &'a crate::LinkGrammarParser,
        module: &str,
        globals: HashMap<String, AstType>,
        types: HashSet<String>,
        aggregates: HashMap<String, AggregateInfo>,
        module_names: Vec<String>,
        phrases: Vec<PhraseBinding>,
        function_returns: HashMap<String, Option<AstType>>,
    ) -> Self {
        Self {
            parser,
            module: module.to_string(),
            globals,
            types,
            aggregates,
            module_names,
            phrases,
            function_returns,
            scopes: vec![Scope::new()],
            discourse: DiscourseStack::new(),
            paraphrase_entries: Vec::new(),
            active_entry: None,
            synthetic_depth: 0,
            trace: Vec::new(),
            errors: Vec::new(),
            temp_counter: 0,
            loop_depth: 0,
            bytes_lengths: HashMap::new(),
            active_loop_var: None,
            last_check: None,
            implicit_counter: None,
            largest_tracker: None,
            print_tracker: None,
            running_total: None,
            named_counters: HashMap::new(),
            move_floor: None,
            last_iteration_var: None,
            last_list: None,
            adjacent: None,
        }
    }

    pub(crate) fn span(&self, span: Span) -> tc_typeck::Span {
        type_span(&self.module, span)
    }

    pub(crate) fn error(
        &self,
        span: Span,
        kind: SemanticErrorKind,
        message: impl Into<String>,
    ) -> SemanticError {
        semantic_error(&self.module, span, kind, message.into())
    }

    pub(super) fn push_scope(&mut self, kind: ScopeKind) {
        self.scopes.push(Scope::new());
        self.discourse.push_scope(kind);
    }

    pub(super) fn pop_scope(&mut self) {
        if self.scopes.len() > 1 {
            self.scopes.pop();
        }
        self.discourse.pop_scope();
    }

    pub(super) fn declare(&mut self, name: &str, ty: AstType) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.vars.insert(name.to_string(), ty);
        }
    }

    pub(crate) fn declare_temp(&mut self, name: &str, ty: AstType) {
        self.declare(name, ty);
    }

    pub(super) fn referent_meta(&self, ty: &AstType) -> (ReferentKind, Option<String>, bool) {
        match ty {
            AstType::Named { name, .. } if self.types.contains(name) => {
                (ReferentKind::Aggregate, Some(name.clone()), false)
            }
            AstType::Ptr(inner) => match inner.as_ref() {
                AstType::Named { name, .. } if self.types.contains(name) => {
                    (ReferentKind::Aggregate, Some(name.clone()), true)
                }
                _ => (ReferentKind::Scalar, None, false),
            },
            AstType::Array(..) => (ReferentKind::Array, None, false),
            _ => (ReferentKind::Scalar, None, false),
        }
    }

    pub(super) fn introduce_name(&mut self, name: &str, ty: &AstType, span: Span) {
        let (kind, type_name, is_ptr) = self.referent_meta(ty);
        self.discourse
            .introduce(name, Some(ty.clone()), type_name, is_ptr, kind, span);
    }

    pub(super) fn touch_name(&mut self, name: &str, span: Span) {
        if self.discourse.touch(name) {
            return;
        }
        if let Some(ty) = self.globals.get(name).cloned() {
            self.introduce_name(name, &ty, span);
        }
    }

    pub(crate) fn touch_target_text(&mut self, text: &str, span: Span) {
        let canonical = canonical_assignment_target(text);
        if let Some(token) = tokenize(&canonical)
            .into_iter()
            .find(|token| token.kind == TokenKind::Word)
        {
            let name = token.lower.clone();
            if self.lookup(&name).is_some() || self.globals.contains_key(&name) {
                self.touch_name(&name, span);
            }
        }
    }

    pub(super) fn push_paraphrase(
        &mut self,
        sentence: &RawSentence,
        verdict: IntentVerdict,
        canonical: String,
    ) {
        let entry = ParaphraseEntry {
            span: sentence.span,
            original: sentence.text.clone(),
            verdict,
            canonical,
            resolutions: Vec::new(),
        };
        self.paraphrase_entries.push(entry);
        self.active_entry = Some(self.paraphrase_entries.len() - 1);
    }

    pub(super) fn add_annotation(&mut self, annotation: String) {
        if let Some(index) = self.active_entry {
            if let Some(entry) = self.paraphrase_entries.get_mut(index) {
                if !entry.resolutions.contains(&annotation) {
                    entry.resolutions.push(annotation);
                }
            }
        }
    }

    pub(super) fn set_canonical(&mut self, canonical: String) {
        if self.synthetic_depth > 0 {
            return;
        }
        if let Some(index) = self.active_entry {
            if let Some(entry) = self.paraphrase_entries.get_mut(index) {
                entry.canonical = canonical;
            }
        }
    }

    pub(super) fn anaphor_error(
        &self,
        error: &crate::discourse::AnaphorError,
        span: Span,
    ) -> SemanticError {
        let _ = span;
        self.error(span, SemanticErrorKind::Lowering, error.to_string())
    }

    pub(super) fn resolve_it(&mut self, span: Span) -> Result<ResolvedAnaphor, SemanticError> {
        match self.discourse.resolve_it(span) {
            Ok(resolved) => {
                self.add_annotation(resolved.annotation.clone());
                Ok(resolved)
            }
            Err(error) => Err(self.anaphor_error(&error, span)),
        }
    }

    pub(super) fn resolve_its_field(
        &mut self,
        field: &str,
        span: Span,
    ) -> Result<ResolvedFieldAnaphor, SemanticError> {
        match self
            .discourse
            .resolve_its_field(field, &self.aggregates, span)
        {
            Ok(resolved) => {
                self.add_annotation(resolved.annotation.clone());
                Ok(resolved)
            }
            Err(error) => Err(self.anaphor_error(&error, span)),
        }
    }

    pub(super) fn resolve_definite(
        &mut self,
        type_word: &str,
        span: Span,
    ) -> Result<ResolvedAnaphor, SemanticError> {
        match self.discourse.resolve_definite_type(type_word, span) {
            Ok(resolved) => {
                self.add_annotation(resolved.annotation.clone());
                Ok(resolved)
            }
            Err(error) => Err(self.anaphor_error(&error, span)),
        }
    }

    pub(super) fn resolve_appositive(
        &mut self,
        type_word: &str,
        var_name: &str,
        span: Span,
    ) -> Result<ResolvedAnaphor, SemanticError> {
        match self.discourse.resolve_appositive(type_word, var_name, span) {
            Ok(resolved) => {
                self.add_annotation(resolved.annotation.clone());
                Ok(resolved)
            }
            Err(error) => Err(self.anaphor_error(&error, span)),
        }
    }

    pub(super) fn is_declared_type(&self, word: &str) -> bool {
        self.types.contains(word)
    }

    pub(super) fn token_at(tokens: &[Token], index: usize) -> Option<&Token> {
        tokens.get(index)
    }

    /// Rewrites `it`, `its <field>`, `the <type>` and appositive noun phrases
    /// into canonical variable references, recording each resolution.
    pub(super) fn rewrite_known_names(&self, text: &str) -> String {
        let mut names = self
            .scopes
            .iter()
            .flat_map(|scope| scope.vars.keys())
            .chain(self.globals.keys())
            .filter(|name| name.contains('_'))
            .cloned()
            .collect::<Vec<_>>();
        names.sort_by(|left, right| {
            right
                .matches('_')
                .count()
                .cmp(&left.matches('_').count())
                .then_with(|| right.len().cmp(&left.len()))
        });
        names.dedup();
        if names.is_empty() {
            return text.to_string();
        }
        let mappings = names
            .into_iter()
            .filter_map(|name| {
                let words = name.split('_').map(ToString::to_string).collect::<Vec<_>>();
                (2..=4).contains(&words.len()).then_some((words, name))
            })
            .collect::<Vec<_>>();
        if mappings.is_empty() {
            return text.to_string();
        }
        let tokens = tokenize(text);
        let mut output = String::new();
        let mut cursor = 0;
        let mut index = 0;
        while index < tokens.len() {
            let token = &tokens[index];
            if token.kind != TokenKind::Word {
                index += 1;
                continue;
            }
            let mut best: Option<(usize, String)> = None;
            for (words, canonical) in &mappings {
                let length = words.len();
                if index + length > tokens.len() {
                    continue;
                }
                let slice = &tokens[index..index + length];
                if !slice
                    .iter()
                    .all(|candidate| candidate.kind == TokenKind::Word)
                    || !slice
                        .windows(2)
                        .all(|pair| text[pair[0].end..pair[1].start].trim().is_empty())
                {
                    continue;
                }
                if slice
                    .iter()
                    .map(|candidate| candidate.lower.as_str())
                    .eq(words.iter().map(String::as_str))
                {
                    best = Some((length, canonical.clone()));
                    break;
                }
            }
            if let Some((length, canonical)) = best {
                output.push_str(&text[cursor..token.start]);
                output.push_str(&canonical);
                cursor = tokens[index + length - 1].end;
                index += length;
            } else {
                index += 1;
            }
        }
        output.push_str(&text[cursor..]);
        output
    }

    pub(super) fn replace_value_referent_phrases(
        &mut self,
        text: &str,
        span: Span,
    ) -> Result<String, SemanticError> {
        let mut current = text.to_string();
        loop {
            let mut replaced = false;
            if let Some(context) = self.adjacent.clone() {
                for (phrase, value) in [
                    ("the left number", &context.left),
                    ("the left one", &context.left),
                    ("the left value", &context.left),
                    ("the right number", &context.right),
                    ("the right one", &context.right),
                    ("the right value", &context.right),
                ] {
                    if let Some((start, end)) =
                        crate::textutil::find_top_level_phrase(&current, phrase)
                    {
                        current = format!("{}{}{}", &current[..start], value, &current[end..]);
                        self.touch_name(value, span);
                        replaced = true;
                        break;
                    }
                }
            }
            if replaced {
                continue;
            }
            if let Some(list) = self.last_list.clone() {
                for phrase in [
                    "the remaining numbers in order",
                    "the remaining items in order",
                    "the remaining values in order",
                    "the numbers in order",
                    "the items in order",
                    "the values in order",
                    "the remaining numbers",
                    "the remaining items",
                    "the remaining values",
                    "the numbers that remain",
                    "the items that remain",
                    "the list",
                    "the numbers",
                    "the items",
                    "the values",
                ] {
                    if let Some((start, end)) =
                        crate::textutil::find_top_level_phrase(&current, phrase)
                    {
                        current = format!("{}{}{}", &current[..start], list, &current[end..]);
                        self.touch_name(&list, span);
                        replaced = true;
                        break;
                    }
                }
            }
            if replaced {
                continue;
            }
            if let Some(var) = self.last_iteration_var.clone() {
                for phrase in [
                    "the stopping number",
                    "that stopping number",
                    "the stop number",
                    "the number where you stopped",
                    "the number at which you stopped",
                ] {
                    if let Some((start, end)) =
                        crate::textutil::find_top_level_phrase(&current, phrase)
                    {
                        current = format!("{}{}{}", &current[..start], var, &current[end..]);
                        self.touch_name(&var, span);
                        replaced = true;
                        break;
                    }
                }
            }
            if replaced {
                continue;
            }
            if let Some(total) = self.running_total.clone() {
                for phrase in [
                    "the final running total",
                    "the running total",
                    "a running total",
                    "that running total",
                    "the final total",
                    "that total",
                    "the running sum",
                    "a running sum",
                ] {
                    if let Some((start, end)) =
                        crate::textutil::find_top_level_phrase(&current, phrase)
                    {
                        current = format!("{}{}{}", &current[..start], total, &current[end..]);
                        replaced = true;
                        break;
                    }
                }
            }
            if replaced {
                continue;
            }
            for phrase in [
                "that largest value",
                "the largest value",
                "the largest one",
                "that maximum value",
                "the maximum value",
                "the biggest value",
                "that biggest value",
                "that smallest value",
                "the smallest value",
                "the minimum value",
                "that minimum value",
            ] {
                if let Some((start, end)) = crate::textutil::find_top_level_phrase(&current, phrase)
                {
                    let Some(tracker) = self.largest_tracker.clone() else {
                        return Err(self.error(
                            span,
                            SemanticErrorKind::Lowering,
                            format!(
                                "UNRESOLVED_ANAPHOR: '{phrase}' has no preceding `keep track of the largest`"
                            ),
                        ));
                    };
                    current = format!("{}{}{}", &current[..start], tracker.name, &current[end..]);
                    self.touch_name(&tracker.name, span);
                    replaced = true;
                    break;
                }
            }
            if replaced {
                continue;
            }
            for phrase in [
                "the final value",
                "the resulting value",
                "the new value",
                "the transformed value",
                "the updated value",
                "the current value",
                "the final number",
                "the final result",
            ] {
                if let Some((start, end)) = crate::textutil::find_top_level_phrase(&current, phrase)
                {
                    let Some(target) = self
                        .active_loop_var
                        .clone()
                        .or_else(|| self.discourse.last_result())
                    else {
                        return Err(self.error(
                            span,
                            SemanticErrorKind::Lowering,
                            format!(
                                "UNRESOLVED_ANAPHOR: '{phrase}' has no active loop value in scope"
                            ),
                        ));
                    };
                    current = format!("{}{}{}", &current[..start], target, &current[end..]);
                    self.touch_name(&target, span);
                    replaced = true;
                    break;
                }
            }
            if !replaced {
                break;
            }
        }
        Ok(current)
    }

    pub(super) fn resolve_anaphors(
        &mut self,
        text: &str,
        span: Span,
    ) -> Result<String, SemanticError> {
        let rewritten = self.replace_value_referent_phrases(text, span)?;
        let rewritten = self.rewrite_known_names(&rewritten);
        let text = rewritten.as_str();
        let tokens = tokenize(text);
        let mut replacements: Vec<(usize, usize, String)> = Vec::new();
        let mut index = 0;
        while index < tokens.len() {
            let token = &tokens[index];
            if token.kind != TokenKind::Word {
                index += 1;
                continue;
            }
            if token.is("it") {
                let negation = Self::token_at(&tokens, index + 1)
                    .is_some_and(|value| value.is("is"))
                    && Self::token_at(&tokens, index + 2).is_some_and(|value| value.is("false"))
                    && Self::token_at(&tokens, index + 3).is_some_and(|value| value.is("that"));
                if negation {
                    index += 1;
                    continue;
                }
                let resolved = self.resolve_it(span)?;
                replacements.push((token.start, token.end, resolved.target_var));
                index += 1;
                continue;
            }
            if token.is("its") {
                let Some(field) =
                    Self::token_at(&tokens, index + 1).filter(|value| value.is_word())
                else {
                    return Err(self.error(
                        span,
                        SemanticErrorKind::Lowering,
                        "UNRESOLVED_ANAPHOR: 'its' requires a field name (e.g. `its x`)",
                    ));
                };
                let resolved = self.resolve_its_field(&field.lower, span)?;
                let _ = resolved.is_ptr;
                replacements.push((
                    token.start,
                    field.end,
                    format!("{}.{}", resolved.base_var, resolved.field),
                ));
                index += 2;
                continue;
            }
            if token.is("the") {
                let Some(next) = Self::token_at(&tokens, index + 1) else {
                    index += 1;
                    continue;
                };
                if next.kind != TokenKind::Word {
                    index += 1;
                    continue;
                }
                let type_word = next.lower.clone();
                if type_word == "result"
                    && self.lookup("result").is_none()
                    && !self.globals.contains_key("result")
                    && let Some(last) = self.discourse.last_result()
                {
                    replacements.push((token.start, next.end, last));
                    index += 2;
                    continue;
                }
                if self.is_declared_type(&type_word) {
                    let after = Self::token_at(&tokens, index + 2);
                    let appositive_name = after
                        .filter(|value| value.is_word())
                        .map(|value| value.lower.clone());
                    let appositive_followed_by_field = appositive_name.as_ref().is_some_and(|_| {
                        Self::token_at(&tokens, index + 3)
                            .is_some_and(|value| value.text == "'s" || value.text == ".")
                    });
                    let appositive_at_end = appositive_name.is_some()
                        && Self::token_at(&tokens, index + 3).is_none_or(|value| {
                            value.kind == TokenKind::Symbol
                                && matches!(value.text.as_str(), "," | "." | ":" | ";")
                        });
                    if appositive_followed_by_field || appositive_at_end {
                        let var_name = appositive_name.unwrap_or_default();
                        let resolved = self.resolve_appositive(&type_word, &var_name, span)?;
                        replacements.push((
                            token.start,
                            after.map_or(next.end, |value| value.end),
                            resolved.target_var,
                        ));
                        index += 3;
                        continue;
                    }
                    let resolved = self.resolve_definite(&type_word, span)?;
                    replacements.push((token.start, next.end, resolved.target_var));
                    index += 2;
                    continue;
                }
                if Self::token_at(&tokens, index + 2)
                    .is_some_and(|value| value.text == "'s" || value.text == ".")
                {
                    let resolved = self.resolve_definite(&type_word, span)?;
                    replacements.push((token.start, next.end, resolved.target_var));
                    index += 2;
                    continue;
                }
                // A definite article before an ordinary identifier is simply
                // dropped: `the salary` denotes the variable `salary`.
                replacements.push((token.start, token.end, String::new()));
                index += 1;
                continue;
            }
            index += 1;
        }
        if replacements.is_empty() {
            return Ok(text.trim().to_string());
        }
        replacements.sort_by_key(|(start, _, _)| *start);
        let mut output = String::with_capacity(text.len());
        let mut cursor = 0;
        for (start, end, replacement) in replacements {
            if start < cursor {
                continue;
            }
            output.push_str(&text[cursor..start]);
            output.push_str(&replacement);
            cursor = end;
        }
        output.push_str(&text[cursor..]);
        Ok(output.trim().to_string())
    }

    pub(super) fn lookup(&self, name: &str) -> Option<&AstType> {
        self.scopes
            .iter()
            .rev()
            .find_map(|scope| scope.vars.get(name))
            .or_else(|| self.globals.get(name))
    }

    pub(super) fn current_scope_contains(&self, name: &str) -> bool {
        self.scopes
            .last()
            .is_some_and(|scope| scope.vars.contains_key(name))
    }

    pub(super) fn expr_env(&self) -> ExprEnv {
        let mut locals = HashMap::new();
        for scope in &self.scopes {
            for (name, ty) in &scope.vars {
                locals.insert(name.clone(), ty.clone());
            }
        }
        ExprEnv {
            locals,
            globals: self.globals.clone(),
            aggregates: self.aggregates.clone(),
            modules: self.module_names.clone(),
        }
    }

    pub(crate) fn next_temp(&mut self, label: &str) -> String {
        self.temp_counter += 1;
        format!("__{label}_{}", self.temp_counter)
    }

    pub(super) fn trace_sentence(
        &mut self,
        sentence: &RawSentence,
        clause: &ClauseTree,
        phrase: Option<&PhraseMatch>,
        verdict: IntentVerdict,
    ) -> IntentVerdict {
        let _ = phrase;
        let mut predicate = None;
        let mut concept = None;
        let mut class_id = None;
        let mut synset_id = None;
        let mut lemma = clause.verb.as_ref().map(|verb| verb.lemma.clone());
        if let Some(verb) = &clause.verb {
            let prep = clause.prep_phrases.first().map(|value| value.prep.as_str());
            if let Some(resolution) = classify_verb(&verb.raw, prep) {
                lemma = Some(resolution.lemma);
                predicate = resolution.predicate;
                concept = resolution.concept;
                class_id = resolution.class_id;
                synset_id = resolution.synset_id;
            }
        }
        self.trace.push(SentenceTrace {
            module: self.module.clone(),
            line: sentence.span.line,
            col: sentence.span.col,
            text: sentence.text.clone(),
            fillers: sentence.fillers.clone(),
            intent: verdict.clone(),
            lg_used: clause.lg_used,
            lg_words: clause.lg_words,
            lg_links: clause.lg_links,
            voice: clause.voice,
            verb: clause.verb.as_ref().map(|verb| verb.raw.clone()),
            lemma,
            predicate,
            concept,
            class_id,
            synset_id,
        });
        verdict
    }
}

/// Runs the declaration and lowering passes for one module.
#[allow(clippy::too_many_arguments)]
pub(crate) fn lower_module(
    module: ModuleJob,
    parser: &crate::LinkGrammarParser,
    module_names: Vec<String>,
    phrases: Vec<PhraseBinding>,
    external_returns: HashMap<String, Option<AstType>>,
    external_types: HashSet<String>,
    external_aggregates: HashMap<String, AggregateInfo>,
    is_entry: bool,
) -> (
    AstModule,
    Vec<SentenceTrace>,
    Vec<SemanticError>,
    ParaphraseDoc,
) {
    let mut module_ast = module.module;
    let globals = module_ast
        .globals
        .iter()
        .map(|global| (global.name.clone(), global.ty.clone()))
        .collect::<HashMap<_, _>>();
    let mut types = external_types;
    types.extend(
        module_ast
            .structs
            .iter()
            .map(|value| value.name.clone())
            .chain(module_ast.unions.iter().map(|value| value.name.clone())),
    );
    let mut aggregates = external_aggregates;
    for definition in &module_ast.structs {
        aggregates.insert(
            definition.name.clone(),
            AggregateInfo {
                type_params: definition.type_params.clone(),
                fields: definition.fields.iter().cloned().collect(),
            },
        );
    }
    for definition in &module_ast.unions {
        aggregates.insert(
            definition.name.clone(),
            AggregateInfo {
                type_params: definition.type_params.clone(),
                fields: definition.fields.iter().cloned().collect(),
            },
        );
    }
    let mut function_returns = external_returns;
    for function in &module_ast.functions {
        function_returns.insert(
            function.name.clone(),
            (!type_contains_params(&function.ret, &function.type_params))
                .then(|| function.ret.clone()),
        );
    }
    let mut lowerer = Lowerer::new(
        parser,
        &module_ast.name,
        globals,
        types,
        aggregates,
        module_names,
        phrases,
        function_returns,
    );
    for job in module.functions {
        let function = &module_ast.functions[job.index];
        lowerer.scopes = vec![Scope::new()];
        lowerer.bytes_lengths.clear();
        lowerer.discourse = DiscourseStack::new();
        lowerer.push_scope(ScopeKind::Function);
        for (name, ty) in &function.params {
            lowerer.declare(name, ty.clone());
            lowerer.introduce_name(name, ty, Span::new(function.span.line, function.span.col));
        }
        let body = lowerer.lower_items(&job.body, 0).unwrap_or_else(|error| {
            lowerer.errors.push(error);
            Vec::new()
        });
        lowerer.pop_scope();
        module_ast.functions[job.index].body = body;
    }
    let has_main = module_ast
        .functions
        .iter()
        .any(|function| function.name == "main");
    if !module.executable.is_empty() {
        if has_main {
            lowerer.errors.push(semantic_error(
                &module_ast.name,
                Span::new(1, 1),
                SemanticErrorKind::Lowering,
                "top-level statements cannot be combined with an explicit `main` function",
            ));
        } else {
            lowerer.scopes = vec![Scope::new()];
            lowerer.discourse = DiscourseStack::new();
            lowerer.push_scope(ScopeKind::ModuleTopLevel);
            let mut body = lowerer
                .lower_items(&module.executable, 0)
                .unwrap_or_else(|error| {
                    lowerer.errors.push(error);
                    Vec::new()
                });
            lowerer.pop_scope();
            if !body
                .iter()
                .any(|statement| matches!(statement, AstStmt::Return(..)))
            {
                body.push(AstStmt::Return(
                    Some(AstExpr::ConstInt(0, None)),
                    tc_typeck::Span::new(&module_ast.name, 1, 1),
                ));
            }
            module_ast.functions.push(AstFn {
                vis: Visibility::Private,
                name: "main".to_string(),
                type_params: Vec::new(),
                params: Vec::new(),
                ret: AstType::I64,
                phrases: Vec::new(),
                body,
                span: tc_typeck::Span::new(&module_ast.name, 1, 1),
            });
        }
    } else if is_entry && !has_main {
        module_ast.functions.push(AstFn {
            vis: Visibility::Private,
            name: "main".to_string(),
            type_params: Vec::new(),
            params: Vec::new(),
            ret: AstType::I64,
            phrases: Vec::new(),
            body: vec![AstStmt::Return(
                Some(AstExpr::ConstInt(0, None)),
                tc_typeck::Span::new(&module_ast.name, 1, 1),
            )],
            span: tc_typeck::Span::new(&module_ast.name, 1, 1),
        });
    }
    let doc = ParaphraseDoc {
        module: module_ast.name.clone(),
        entries: lowerer.paraphrase_entries,
    };
    (module_ast, lowerer.trace, lowerer.errors, doc)
}

impl ValueContext for Lowerer<'_> {
    fn match_value_phrase(
        &mut self,
        text: &str,
        span: Span,
    ) -> Result<Option<PhraseMatch>, SemanticError> {
        self.match_phrase_typed(text, span)
    }

    fn value_phrase_expression(
        &mut self,
        phrase: &PhraseMatch,
        span: Span,
    ) -> Result<AstExpr, SemanticError> {
        self.phrase_expression(phrase, span)
    }

    fn value_expression(&mut self, text: &str, span: Span) -> Result<AstExpr, SemanticError> {
        self.parse_expr(text, span)
    }

    fn value_atom(&mut self, text: &str, span: Span) -> Result<AstExpr, SemanticError> {
        self.parse_value(text, span)
    }

    fn length_expression(
        &mut self,
        value: AstExpr,
        span: Span,
    ) -> Result<Option<AstExpr>, SemanticError> {
        if let AstExpr::BytesLit(bytes) = &value {
            return Ok(Some(AstExpr::ConstInt(
                i64::try_from(bytes.len()).unwrap_or(i64::MAX),
                None,
            )));
        }
        if let AstExpr::Var(name) = &value {
            if let Some(length) = self.bytes_lengths.get(name) {
                return Ok(Some(AstExpr::ConstInt(
                    i64::try_from(*length).unwrap_or(i64::MAX),
                    None,
                )));
            }
        }
        let Some(ty) = self.shallow_type_opt(&value) else {
            return Ok(None);
        };
        let aggregate = match &ty {
            AstType::Named { name, .. } if name.eq_ignore_ascii_case("list") => true,
            AstType::Named { name, .. } if name.eq_ignore_ascii_case("tcstring") => true,
            AstType::Ptr(inner) => match inner.as_ref() {
                AstType::Named { name, .. } => {
                    name.eq_ignore_ascii_case("list") || name.eq_ignore_ascii_case("tcstring")
                }
                _ => false,
            },
            _ => false,
        };
        if !aggregate {
            return Ok(None);
        }
        match &value {
            AstExpr::Var(name) => {
                if self.globals.contains_key(name) && !self.current_scope_contains(name) {
                    let base_ptr = AstExpr::GlobalAddr {
                        module: None,
                        name: name.clone(),
                    };
                    return Ok(Some(AstExpr::Load(Box::new(AstExpr::FieldAddr {
                        base_ptr: Box::new(base_ptr),
                        field: "len".to_string(),
                    }))));
                }
                let field = format!("{name}'s len");
                let expression = self.parse_expr(&field, span)?;
                Ok(Some(expression))
            }
            _ => Ok(None),
        }
    }

    fn value_error(&self, span: Span, message: &str) -> SemanticError {
        self.error(span, SemanticErrorKind::Lowering, message)
    }

    fn plural_names(&self) -> Vec<String> {
        self.discourse.plural_names()
    }

    fn result_name(&self) -> Option<String> {
        if self.lookup("result").is_some() || self.globals.contains_key("result") {
            None
        } else {
            self.discourse.last_result()
        }
    }

    fn note_anaphor(&mut self, annotation: String) {
        self.add_annotation(annotation);
    }
}
