//! DRS-style discourse referent tracking for English anaphora.
//!
//! The stack mirrors lexical scopes exactly.  Referents introduced inside a
//! branch, loop, or quantifier body are popped when that scope closes, so an
//! anaphor can never bind to a variable that is not lexically accessible.

use std::collections::{HashMap, HashSet};
use std::fmt;

use tc_typeck::AstType;

use crate::Span;
use crate::expr::AggregateInfo;

/// Coarse category of a discourse referent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum ReferentKind {
    /// Scalar variable (integer, float, boolean, pointer, function pointer).
    Scalar,
    /// Aggregate value or pointer to an aggregate.
    Aggregate,
    /// Fixed-size array.
    Array,
    /// Local whose type is not known at introduction time.
    Inferred,
}

/// One addressable discourse referent.
#[derive(Debug, Clone)]
pub(crate) struct Referent {
    /// Canonical lowercase variable name.
    pub name: String,
    /// Known AST type, when available.
    pub ty: Option<AstType>,
    /// Canonical lowercase struct or union name.
    pub type_name: Option<String>,
    /// True when the value is a pointer to an aggregate.
    pub is_ptr_to_aggregate: bool,
    /// Referent category.
    pub kind: ReferentKind,
    /// Monotonic recency stamp.
    pub recency: u64,
    /// Introduction or last-touch span.
    pub span: Span,
}

/// Lexical scope category.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum ScopeKind {
    /// Module-level executable statements.
    ModuleTopLevel,
    /// Function parameter and body scope.
    Function,
    /// Explicit nested block.
    Block,
    /// `If` / `Unless` / `While` condition.
    IfCondition,
    /// `If` true branch.
    IfThen,
    /// `If` false branch.
    IfElse,
    /// Loop body.
    LoopBody,
    /// `For every` body or a local quantifier predicate.
    QuantifierBody,
    /// Statement-local `where` bindings.
    WhereClause,
}

/// One lexical scope of referents.
#[derive(Debug, Clone)]
pub(crate) struct DiscourseScope {
    /// Scope category.
    pub kind: ScopeKind,
    /// Referents introduced directly in this scope.
    pub referents: Vec<Referent>,
}

/// A stack of lexically nested referent scopes.
#[derive(Debug, Clone)]
pub(crate) struct DiscourseStack {
    scopes: Vec<DiscourseScope>,
    next_recency: u64,
    recent_plural: Vec<String>,
    last_result: Option<String>,
}

/// Structured anaphor resolution failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum AnaphorError {
    /// A pronoun has no accessible antecedent.
    UnresolvedPronoun {
        /// Surface form, e.g. `it` or `its x`.
        token: String,
        /// Source span.
        span: Span,
        /// Human-actionable hint.
        hint: String,
    },
    /// `its <field>` cannot be bound to any accessible aggregate field.
    UnresolvedFieldAnaphor {
        /// Requested field name.
        field: String,
        /// Source span.
        span: Span,
        /// Accessible aggregate candidates.
        candidates: Vec<String>,
        /// Human-actionable hint.
        hint: String,
    },
    /// `the <type>` has no accessible instance.
    UnresolvedDefiniteType {
        /// Canonical type name.
        type_name: String,
        /// Source span.
        span: Span,
        /// Human-actionable hint.
        hint: String,
    },
    /// `the <type> <name>` names a variable of a different type.
    AppositiveTypeMismatch {
        /// Canonical expected type.
        expected_type: String,
        /// Canonical actual type.
        actual_type: String,
        /// Variable name.
        var_name: String,
        /// Source span.
        span: Span,
    },
    /// `the <type> <name>` names an unknown variable.
    UnresolvedAppositive {
        /// Canonical type name.
        type_name: String,
        /// Variable name.
        var_name: String,
        /// Source span.
        span: Span,
    },
}

impl fmt::Display for AnaphorError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnresolvedPronoun { token, hint, .. } => write!(
                formatter,
                "UNRESOLVED_ANAPHOR: '{token}' has no accessible referent in scope ({hint})"
            ),
            Self::UnresolvedFieldAnaphor { field, hint, .. } => write!(
                formatter,
                "UNRESOLVED_ANAPHOR: 'its {field}' could not be resolved to any accessible struct/union field ({hint})"
            ),
            Self::UnresolvedDefiniteType {
                type_name, hint, ..
            } => write!(
                formatter,
                "UNRESOLVED_ANAPHOR: 'the {type_name}' has no accessible instance of '{type_name}' in scope ({hint})"
            ),
            Self::AppositiveTypeMismatch {
                expected_type,
                actual_type,
                var_name,
                ..
            } => write!(
                formatter,
                "ANAPHOR_TYPE_MISMATCH: 'the {expected_type} {var_name}' refers to '{var_name}' of type '{actual_type}'"
            ),
            Self::UnresolvedAppositive {
                type_name,
                var_name,
                ..
            } => write!(
                formatter,
                "UNRESOLVED_ANAPHOR: 'the {type_name} {var_name}' refers to unknown variable '{var_name}'"
            ),
        }
    }
}

/// A resolved pronoun.
#[derive(Debug, Clone)]
pub(crate) struct ResolvedAnaphor {
    /// Bound variable name.
    pub target_var: String,
    /// Paraphrase annotation, e.g. `[it -> cash]`.
    pub annotation: String,
}

/// A resolved possessive field anaphor.
#[derive(Debug, Clone)]
pub(crate) struct ResolvedFieldAnaphor {
    /// Bound aggregate variable name.
    pub base_var: String,
    /// Canonical field name.
    pub field: String,
    /// True when the aggregate is held through a pointer.
    pub is_ptr: bool,
    /// Paraphrase annotation, e.g. `[its x -> p.x]`.
    pub annotation: String,
}

impl Default for DiscourseStack {
    fn default() -> Self {
        Self::new()
    }
}

impl DiscourseStack {
    /// Creates an empty stack.
    pub(crate) fn new() -> Self {
        Self {
            scopes: Vec::new(),
            next_recency: 1,
            recent_plural: Vec::new(),
            last_result: None,
        }
    }

    /// Pushes a fresh lexical scope.
    pub(crate) fn push_scope(&mut self, kind: ScopeKind) {
        self.scopes.push(DiscourseScope {
            kind,
            referents: Vec::new(),
        });
    }

    /// Pops the innermost scope.
    pub(crate) fn pop_scope(&mut self) -> Option<DiscourseScope> {
        self.scopes.pop()
    }

    /// Records the active plural referent group for `their ...`.
    pub(crate) fn set_plural(&mut self, names: Vec<String>) {
        self.recent_plural.clear();
        for name in names {
            let name = name.to_ascii_lowercase();
            if !self.recent_plural.contains(&name) {
                self.recent_plural.push(name);
            }
        }
    }

    /// Returns the active plural referent group.
    pub(crate) fn plural_names(&self) -> Vec<String> {
        self.recent_plural.clone()
    }

    /// Records the most recent assignment/computation target.
    pub(crate) fn set_last_result(&mut self, name: &str) {
        self.last_result = Some(name.to_ascii_lowercase());
    }

    /// Returns the most recent assignment target.
    pub(crate) fn last_result(&self) -> Option<String> {
        self.last_result.clone()
    }

    /// Introduces or refreshes a binding in the innermost scope.
    pub(crate) fn introduce(
        &mut self,
        name: &str,
        ty: Option<AstType>,
        type_name: Option<String>,
        is_ptr_to_aggregate: bool,
        kind: ReferentKind,
        span: Span,
    ) {
        let canon = name.to_ascii_lowercase();
        let recency = self.bump_recency();
        if let Some(scope) = self.scopes.last_mut() {
            if let Some(existing) = scope.referents.iter_mut().find(|r| r.name == canon) {
                existing.ty = ty.or_else(|| existing.ty.clone());
                existing.type_name = type_name.or_else(|| existing.type_name.clone());
                existing.is_ptr_to_aggregate = is_ptr_to_aggregate;
                if kind != ReferentKind::Inferred {
                    existing.kind = kind;
                }
                existing.recency = recency;
                existing.span = span;
                return;
            }
            scope.referents.push(Referent {
                name: canon,
                ty,
                type_name,
                is_ptr_to_aggregate,
                kind,
                recency,
                span,
            });
        }
    }

    /// Refreshes the recency of an accessible binding without leaking scopes.
    pub(crate) fn touch(&mut self, name: &str) -> bool {
        let canon = name.to_ascii_lowercase();
        let recency = self.bump_recency();
        for scope in self.scopes.iter_mut().rev() {
            if let Some(existing) = scope.referents.iter_mut().find(|r| r.name == canon) {
                existing.recency = recency;
                return true;
            }
            if scope.kind == ScopeKind::Function {
                break;
            }
        }
        false
    }

    /// Number of referents introduced directly in the innermost scope.
    pub(crate) fn current_referent_count(&self) -> usize {
        self.scopes.last().map_or(0, |scope| scope.referents.len())
    }

    /// Removes and returns referents introduced after a checkpoint.
    pub(crate) fn take_referents_since(&mut self, count: usize) -> Vec<Referent> {
        self.scopes
            .last_mut()
            .map_or_else(Vec::new, |scope| scope.referents.split_off(count))
    }

    /// Appends referents to the innermost scope (used when leaving `where`).
    pub(crate) fn append_referents(&mut self, referents: Vec<Referent>) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.referents.extend(referents);
        }
    }

    /// Returns accessible referents, innermost shadowing outermost, newest first.
    pub(crate) fn accessible_referents(&self) -> Vec<&Referent> {
        let mut seen = HashSet::new();
        let mut out = Vec::new();
        for scope in self.scopes.iter().rev() {
            for referent in scope.referents.iter().rev() {
                if seen.insert(referent.name.as_str()) {
                    out.push(referent);
                }
            }
            if scope.kind == ScopeKind::Function {
                break;
            }
        }
        out.sort_by_key(|left| std::cmp::Reverse(left.recency));
        out
    }

    /// Finds an accessible referent by canonical name.
    pub(crate) fn referent_named(&self, name: &str) -> Option<&Referent> {
        let canon = name.to_ascii_lowercase();
        self.accessible_referents()
            .into_iter()
            .find(|referent| referent.name == canon)
    }

    fn bump_recency(&mut self) -> u64 {
        let recency = self.next_recency;
        self.next_recency += 1;
        recency
    }
}

impl DiscourseStack {
    /// Resolves the most recent accessible pronoun referent.
    pub(crate) fn resolve_it(&self, span: Span) -> Result<ResolvedAnaphor, AnaphorError> {
        let Some(referent) = self.accessible_referents().into_iter().next() else {
            return Err(AnaphorError::UnresolvedPronoun {
                token: "it".to_string(),
                span,
                hint: "introduce one first, for example: Put 10 in cash.".to_string(),
            });
        };
        Ok(ResolvedAnaphor {
            target_var: referent.name.clone(),
            annotation: format!("[it -> {}]", referent.name),
        })
    }

    /// Resolves `its <field>` to the newest accessible aggregate with that field.
    pub(crate) fn resolve_its_field(
        &self,
        field: &str,
        aggregates: &HashMap<String, AggregateInfo>,
        span: Span,
    ) -> Result<ResolvedFieldAnaphor, AnaphorError> {
        let field = field.to_ascii_lowercase();
        let candidates = self
            .accessible_referents()
            .into_iter()
            .filter(|referent| referent.kind == ReferentKind::Aggregate)
            .collect::<Vec<_>>();
        if candidates.is_empty() {
            return Err(AnaphorError::UnresolvedPronoun {
                token: format!("its {field}"),
                span,
                hint: "no accessible struct or union value exists — create one first, for example: Create a Waypoint called p.".to_string(),
            });
        }
        for referent in &candidates {
            let Some(type_name) = &referent.type_name else {
                continue;
            };
            let Some(info) = aggregates.get(type_name) else {
                continue;
            };
            if info.fields.contains_key(&field) {
                return Ok(ResolvedFieldAnaphor {
                    base_var: referent.name.clone(),
                    field: field.clone(),
                    is_ptr: referent.is_ptr_to_aggregate,
                    annotation: format!("[its {field} -> {}.{field}]", referent.name),
                });
            }
        }
        let available = candidates
            .iter()
            .filter_map(|referent| {
                let type_name = referent.type_name.as_ref()?;
                let info = aggregates.get(type_name)?;
                let mut fields = info.fields.keys().cloned().collect::<Vec<_>>();
                fields.sort();
                Some(format!(
                    "available on '{}': {}",
                    referent.name,
                    fields.join(", ")
                ))
            })
            .collect::<Vec<_>>();
        let hint = if available.is_empty() {
            format!("none of the accessible aggregates declares a field named '{field}'")
        } else {
            format!(
                "none of the accessible aggregates declares a field named '{field}'; {}",
                available.join("; ")
            )
        };
        Err(AnaphorError::UnresolvedFieldAnaphor {
            field: field.clone(),
            span,
            candidates: candidates
                .iter()
                .map(|referent| referent.name.clone())
                .collect(),
            hint,
        })
    }

    /// Resolves `the <type>` to a variable named `<type>` or the newest instance.
    pub(crate) fn resolve_definite_type(
        &self,
        type_word: &str,
        span: Span,
    ) -> Result<ResolvedAnaphor, AnaphorError> {
        let type_word = type_word.to_ascii_lowercase();
        if let Some(referent) = self.referent_named(&type_word) {
            return Ok(ResolvedAnaphor {
                target_var: referent.name.clone(),
                annotation: format!("[the {type_word} -> {}]", referent.name),
            });
        }
        let referent = self
            .accessible_referents()
            .into_iter()
            .find(|candidate| candidate.type_name.as_deref() == Some(type_word.as_str()));
        let Some(referent) = referent else {
            return Err(AnaphorError::UnresolvedDefiniteType {
                type_name: type_word.clone(),
                span,
                hint: format!("create one first, for example: Create a {type_word} called p."),
            });
        };
        Ok(ResolvedAnaphor {
            target_var: referent.name.clone(),
            annotation: format!("[the {type_word} -> {}]", referent.name),
        })
    }

    /// Resolves the appositive `the <type> <name>` and verifies the type.
    pub(crate) fn resolve_appositive(
        &self,
        type_name: &str,
        var_name: &str,
        span: Span,
    ) -> Result<ResolvedAnaphor, AnaphorError> {
        let type_name = type_name.to_ascii_lowercase();
        let var_name = var_name.to_ascii_lowercase();
        let Some(referent) = self.referent_named(&var_name) else {
            return Err(AnaphorError::UnresolvedAppositive {
                type_name,
                var_name,
                span,
            });
        };
        let actual = referent
            .type_name
            .clone()
            .unwrap_or_else(|| "scalar".to_string());
        if actual != type_name {
            return Err(AnaphorError::AppositiveTypeMismatch {
                expected_type: type_name,
                actual_type: actual,
                var_name,
                span,
            });
        }
        Ok(ResolvedAnaphor {
            target_var: referent.name.clone(),
            annotation: format!("[the {actual} {var_name} -> {var_name}]"),
        })
    }
}
