//! Declaration parsing and the single unified Core-7 statement lowerer.

use std::collections::{HashMap, HashSet};

use tc_lexicon::{SemanticConcept, VerbPredicate, classify_verb, prep_role};
use tc_typeck::{
    AstExpr, AstFn, AstGlobal, AstModule, AstStmt, AstStruct, AstType, AstUnion, Import,
    PhrasePart, PhrasePattern, Visibility,
};

use crate::expr::{ExprEnv, parse_address, parse_expression, parse_store_address, parse_type_text};
use crate::intent::{IntentVerdict, classify};
use crate::phrases::{PhraseBinding, PhraseMatch, match_all};
use crate::textutil::{
    find_top_level_symbol, find_top_level_word, first_word, is_lvalue_text, split_top_level,
    split_type_list, split_where, starts_with_word,
};
use crate::token::{Token, TokenKind, decode_string_token, tokenize};
use crate::{
    ClauseTree, RawItem, RawSentence, SemanticError, SemanticErrorKind, SentenceTrace, SourceFile,
    Span, Voice, analyze_clause, segment_source,
};

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

fn lower_name(name: &str) -> String {
    name.to_ascii_lowercase()
}

fn declaration_kind(text: &str) -> Option<&'static str> {
    let tokens = tokenize(text);
    let mut words = tokens
        .iter()
        .filter(|token| token.kind == TokenKind::Word)
        .map(|token| token.lower.as_str());
    let mut first = words.next()?;
    if first == "define" {
        first = words.next()?;
    }
    if matches!(first, "public" | "private") {
        first = words.next()?;
    }
    match first {
        "struct" => Some("struct"),
        "union" => Some("union"),
        "global" => Some("global"),
        "function" => Some("function"),
        "import" => Some("import"),
        _ => None,
    }
}

fn parse_visibility(text: &str) -> Visibility {
    let lower = text.to_ascii_lowercase();
    if lower.split_whitespace().any(|word| word == "public") {
        Visibility::Public
    } else {
        Visibility::Private
    }
}

fn parse_import(text: &str, module: &str, span: Span) -> Result<Import, SemanticError> {
    let tokens = tokenize(text);
    let mut index = 1;
    let module_name = tokens
        .get(index)
        .filter(|token| token.is_word())
        .map(|token| token.lower.clone())
        .ok_or_else(|| {
            semantic_error(
                module,
                span,
                SemanticErrorKind::Declaration,
                "import requires a module name",
            )
        })?;
    index += 1;
    let alias = if tokens.get(index).is_some_and(|token| token.is("as")) {
        index += 1;
        let alias = tokens
            .get(index)
            .filter(|token| token.is_word())
            .map(|token| token.lower.clone())
            .ok_or_else(|| {
                semantic_error(
                    module,
                    span,
                    SemanticErrorKind::Declaration,
                    "import alias is missing",
                )
            })?;
        Some(alias)
    } else {
        None
    };
    Ok(Import {
        module: module_name,
        alias,
        span: type_span(module, span),
    })
}

fn split_as(text: &str) -> Option<(String, String)> {
    let (start, end) = find_top_level_word(text, "as")?;
    let left = text[..start].trim();
    let right = text[end..].trim();
    if left.is_empty() || right.is_empty() {
        return None;
    }
    Some((left.to_string(), right.to_string()))
}

fn parse_fields(
    text: &str,
    module: &str,
    span: Span,
) -> Result<Vec<(String, AstType)>, SemanticError> {
    let mut fields = Vec::new();
    let mut pieces = Vec::new();
    for comma_piece in split_type_list(text, ",") {
        pieces.extend(split_type_list(&comma_piece, "and"));
    }
    for piece in pieces {
        let Some((name, ty_text)) = split_as(&piece) else {
            return Err(semantic_error(
                module,
                span,
                SemanticErrorKind::Declaration,
                format!("field declaration `{piece}` must use `<name> as <type>`"),
            ));
        };
        let name = lower_name(name.trim());
        if !name
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '_')
        {
            return Err(semantic_error(
                module,
                span,
                SemanticErrorKind::Declaration,
                format!("invalid field name `{name}`"),
            ));
        }
        let ty = parse_type_text(ty_text.trim()).map_err(|message| {
            semantic_error(module, span, SemanticErrorKind::Declaration, message)
        })?;
        fields.push((name, ty));
    }
    Ok(fields)
}

fn parse_type_params(text: &str, module: &str, span: Span) -> Result<Vec<String>, SemanticError> {
    let mut params = Vec::new();
    for piece in split_type_list(text, "and") {
        for inner in split_type_list(&piece, ",") {
            let value = inner.trim();
            if value.is_empty() {
                continue;
            }
            if !value
                .chars()
                .all(|character| character.is_ascii_alphanumeric() || character == '_')
            {
                return Err(semantic_error(
                    module,
                    span,
                    SemanticErrorKind::Declaration,
                    format!("invalid type parameter `{value}`"),
                ));
            }
            params.push(lower_name(value));
        }
    }
    Ok(params)
}

fn parse_struct_header(
    text: &str,
    module: &str,
    span: Span,
    is_union: bool,
) -> Result<AggregateHeader, SemanticError> {
    let tokens = tokenize(text);
    let mut index = 0;
    while tokens
        .get(index)
        .is_some_and(|token| token.is("define") || token.is("public") || token.is("private"))
    {
        index += 1;
    }
    let keyword = if is_union { "union" } else { "struct" };
    if !tokens.get(index).is_some_and(|token| token.is(keyword)) {
        return Err(semantic_error(
            module,
            span,
            SemanticErrorKind::Declaration,
            format!("expected `{keyword}`"),
        ));
    }
    index += 1;
    let name = tokens
        .get(index)
        .filter(|token| token.is_word())
        .map(|token| lower_name(&token.text))
        .ok_or_else(|| {
            semantic_error(
                module,
                span,
                SemanticErrorKind::Declaration,
                "aggregate name is missing",
            )
        })?;
    index += 1;
    let rest_start = tokens.get(index).map_or(text.len(), |token| token.start);
    let rest = &text[rest_start..];
    let (type_text, field_text) = if let Some((start, end)) = find_top_level_word(rest, "with") {
        (&rest[..start], Some(&rest[end..]))
    } else {
        (rest, None)
    };
    let type_params = if let Some((_, end)) = find_top_level_word(type_text, "of") {
        parse_type_params(type_text[end..].trim(), module, span)?
    } else {
        Vec::new()
    };
    let fields = if let Some(field_text) = field_text {
        parse_fields(field_text, module, span)?
    } else {
        Vec::new()
    };
    Ok((name, type_params, fields))
}

fn extract_phrase_lines(
    items: &mut Vec<RawItem>,
    module: &str,
    span: Span,
) -> Result<Vec<PhrasePattern>, SemanticError> {
    let mut patterns = Vec::new();
    let mut remaining = Vec::new();
    for item in std::mem::take(items) {
        match item {
            RawItem::Sentence(sentence)
                if sentence.text.to_ascii_lowercase().starts_with("phrase:") =>
            {
                patterns.push(parse_phrase_pattern(&sentence.text, module, span)?);
            }
            other => remaining.push(other),
        }
    }
    *items = remaining;
    Ok(patterns)
}

fn parse_phrase_pattern(
    text: &str,
    module: &str,
    span: Span,
) -> Result<PhrasePattern, SemanticError> {
    let tokens = tokenize(text);
    let literal = tokens
        .iter()
        .find(|token| token.kind == TokenKind::String)
        .map(|token| token.text.clone())
        .ok_or_else(|| {
            semantic_error(
                module,
                span,
                SemanticErrorKind::Declaration,
                "Phrase: requires a quoted pattern",
            )
        })?;
    let decoded = decode_string_token(&literal).ok_or_else(|| {
        semantic_error(
            module,
            span,
            SemanticErrorKind::Declaration,
            "malformed phrase string",
        )
    })?;
    let pattern_text = String::from_utf8(decoded).map_err(|_| {
        semantic_error(
            module,
            span,
            SemanticErrorKind::Declaration,
            "phrase pattern must be UTF-8",
        )
    })?;
    let mut parts = Vec::new();
    for word in pattern_text.split_whitespace() {
        if let Some(param) = word
            .strip_prefix('<')
            .and_then(|value| value.strip_suffix('>'))
        {
            let param = lower_name(param);
            if param.is_empty() {
                return Err(semantic_error(
                    module,
                    span,
                    SemanticErrorKind::Declaration,
                    "empty phrase parameter",
                ));
            }
            parts.push(PhrasePart::Param(param));
        } else {
            parts.push(PhrasePart::Word(lower_name(word)));
        }
    }
    if parts.is_empty() {
        return Err(semantic_error(
            module,
            span,
            SemanticErrorKind::Declaration,
            "empty phrase pattern",
        ));
    }
    Ok(PhrasePattern { parts })
}

fn parse_function_header(text: &str, module: &str, span: Span) -> Result<AstFn, SemanticError> {
    let tokens = tokenize(text);
    let mut index = 0;
    while tokens
        .get(index)
        .is_some_and(|token| token.is("define") || token.is("public") || token.is("private"))
    {
        index += 1;
    }
    if !tokens.get(index).is_some_and(|token| token.is("function")) {
        return Err(semantic_error(
            module,
            span,
            SemanticErrorKind::Declaration,
            "expected `function`",
        ));
    }
    index += 1;
    let name = tokens
        .get(index)
        .filter(|token| token.is_word())
        .map(|token| lower_name(&token.text))
        .ok_or_else(|| {
            semantic_error(
                module,
                span,
                SemanticErrorKind::Declaration,
                "function name is missing",
            )
        })?;
    index += 1;
    let mut type_params = Vec::new();
    let mut params = Vec::new();
    let mut ret = AstType::Void;
    while index < tokens.len() {
        if tokens[index].is("of") {
            index += 1;
            let start = tokens.get(index).map_or(text.len(), |token| token.start);
            let mut end = text.len();
            while index < tokens.len() {
                if tokens[index].is("taking") || tokens[index].is("returning") {
                    end = tokens[index].start;
                    break;
                }
                index += 1;
            }
            type_params = parse_type_params(&text[start..end], module, span)?;
            continue;
        }
        if tokens[index].is("taking") {
            index += 1;
            let start = tokens.get(index).map_or(text.len(), |token| token.start);
            let mut end = text.len();
            while index < tokens.len() {
                if tokens[index].is("returning") {
                    end = tokens[index].start;
                    break;
                }
                index += 1;
            }
            let param_text = &text[start..end];
            for piece in split_type_list(param_text, "and") {
                for inner in split_type_list(&piece, ",") {
                    if inner.trim().is_empty() {
                        continue;
                    }
                    let Some((param_name, ty_text)) = split_as(&inner) else {
                        return Err(semantic_error(
                            module,
                            span,
                            SemanticErrorKind::Declaration,
                            format!("parameter `{inner}` must use `<name> as <type>`"),
                        ));
                    };
                    let ty = parse_type_text(ty_text.trim()).map_err(|message| {
                        semantic_error(module, span, SemanticErrorKind::Declaration, message)
                    })?;
                    params.push((lower_name(param_name.trim()), ty));
                }
            }
            continue;
        }
        if tokens[index].is("returning") {
            index += 1;
            let start = tokens.get(index).map_or(text.len(), |token| token.start);
            let ty_text = text[start..].trim();
            ret = parse_type_text(ty_text).map_err(|message| {
                semantic_error(module, span, SemanticErrorKind::Declaration, message)
            })?;
            break;
        }
        index += 1;
    }
    Ok(AstFn {
        vis: parse_visibility(text),
        name,
        type_params,
        params,
        ret,
        phrases: Vec::new(),
        body: Vec::new(),
        span: type_span(module, span),
    })
}

/// Runs the declaration pass over one segmented module.
pub(crate) fn parse_module(source: &SourceFile) -> Result<ModuleJob, Vec<SemanticError>> {
    let module_name = source.module_name();
    let document = segment_source(source);
    let mut ast = AstModule {
        name: module_name.clone(),
        imports: Vec::new(),
        structs: Vec::new(),
        unions: Vec::new(),
        globals: Vec::new(),
        functions: Vec::new(),
    };
    let mut functions = Vec::new();
    let mut executable = Vec::new();
    let mut errors = Vec::new();
    for item in document.items {
        match item {
            RawItem::Sentence(sentence) => {
                let Some(kind) = declaration_kind(&sentence.text) else {
                    executable.push(RawItem::Sentence(sentence));
                    continue;
                };
                let span = sentence.span;
                match kind {
                    "import" => match parse_import(&sentence.text, &module_name, span) {
                        Ok(value) => ast.imports.push(value),
                        Err(error) => errors.push(error),
                    },
                    "global" => match parse_global(&sentence.text, &module_name, span) {
                        Ok(value) => ast.globals.push(value),
                        Err(error) => errors.push(error),
                    },
                    "struct" | "union" => {
                        let is_union = kind == "union";
                        match parse_struct_header(&sentence.text, &module_name, span, is_union) {
                            Ok((name, type_params, fields)) => {
                                if is_union {
                                    ast.unions.push(AstUnion {
                                        vis: parse_visibility(&sentence.text),
                                        name,
                                        type_params,
                                        fields,
                                        span: type_span(&module_name, span),
                                    });
                                } else {
                                    ast.structs.push(AstStruct {
                                        vis: parse_visibility(&sentence.text),
                                        name,
                                        type_params,
                                        fields,
                                        span: type_span(&module_name, span),
                                    });
                                }
                            }
                            Err(error) => errors.push(error),
                        }
                    }
                    "function" => match parse_function_header(&sentence.text, &module_name, span) {
                        Ok(function) => {
                            ast.functions.push(function);
                            functions.push(FunctionJob {
                                index: ast.functions.len() - 1,
                                body: Vec::new(),
                            });
                        }
                        Err(error) => errors.push(error),
                    },
                    _ => executable.push(RawItem::Sentence(sentence)),
                }
            }
            RawItem::Block(block) => {
                let Some(header) = &block.header else {
                    executable.push(RawItem::Block(block));
                    continue;
                };
                let Some(kind) = declaration_kind(&header.text) else {
                    executable.push(RawItem::Block(block));
                    continue;
                };
                let span = header.span;
                match kind {
                    "struct" | "union" => {
                        let is_union = kind == "union";
                        match parse_struct_header(&header.text, &module_name, span, is_union) {
                            Ok((name, type_params, mut fields)) => {
                                for item in &block.items {
                                    if let RawItem::Sentence(field) = item {
                                        match parse_fields(&field.text, &module_name, field.span) {
                                            Ok(mut more) => fields.append(&mut more),
                                            Err(error) => errors.push(error),
                                        }
                                    }
                                }
                                if is_union {
                                    ast.unions.push(AstUnion {
                                        vis: parse_visibility(&header.text),
                                        name,
                                        type_params,
                                        fields,
                                        span: type_span(&module_name, span),
                                    });
                                } else {
                                    ast.structs.push(AstStruct {
                                        vis: parse_visibility(&header.text),
                                        name,
                                        type_params,
                                        fields,
                                        span: type_span(&module_name, span),
                                    });
                                }
                            }
                            Err(error) => errors.push(error),
                        }
                    }
                    "function" => match parse_function_header(&header.text, &module_name, span) {
                        Ok(mut function) => {
                            let mut body = block.items;
                            match extract_phrase_lines(&mut body, &module_name, span) {
                                Ok(patterns) => function.phrases = patterns,
                                Err(error) => errors.push(error),
                            }
                            ast.functions.push(function);
                            functions.push(FunctionJob {
                                index: ast.functions.len() - 1,
                                body,
                            });
                        }
                        Err(error) => errors.push(error),
                    },
                    "global" => match parse_global(&header.text, &module_name, span) {
                        Ok(value) => ast.globals.push(value),
                        Err(error) => errors.push(error),
                    },
                    _ => executable.push(RawItem::Block(block)),
                }
            }
        }
    }
    if errors.is_empty() {
        Ok(ModuleJob {
            module: ast,
            functions,
            executable,
        })
    } else {
        Err(errors)
    }
}

fn parse_global(text: &str, module: &str, span: Span) -> Result<AstGlobal, SemanticError> {
    let tokens = tokenize(text);
    let mut index = 0;
    while tokens
        .get(index)
        .is_some_and(|token| token.is("define") || token.is("public") || token.is("private"))
    {
        index += 1;
    }
    if !tokens.get(index).is_some_and(|token| token.is("global")) {
        return Err(semantic_error(
            module,
            span,
            SemanticErrorKind::Declaration,
            "expected `global`",
        ));
    }
    index += 1;
    let name = tokens
        .get(index)
        .filter(|token| token.is_word())
        .map(|token| lower_name(&token.text))
        .ok_or_else(|| {
            semantic_error(
                module,
                span,
                SemanticErrorKind::Declaration,
                "global name is missing",
            )
        })?;
    index += 1;
    if !tokens.get(index).is_some_and(|token| token.is("as")) {
        return Err(semantic_error(
            module,
            span,
            SemanticErrorKind::Declaration,
            "global requires `as <type>`",
        ));
    }
    index += 1;
    let start = tokens.get(index).map_or(text.len(), |token| token.start);
    let ty = parse_type_text(&text[start..])
        .map_err(|message| semantic_error(module, span, SemanticErrorKind::Declaration, message))?;
    Ok(AstGlobal {
        vis: parse_visibility(text),
        name,
        ty,
        init: None,
        span: type_span(module, span),
    })
}

/// Builds the per-module phrase scope: own functions plus imported public ones.
#[must_use]
pub(crate) fn collect_phrase_bindings(modules: &[ModuleJob]) -> Vec<Vec<PhraseBinding>> {
    let by_name = modules
        .iter()
        .enumerate()
        .map(|(index, module)| (module.module.name.clone(), index))
        .collect::<HashMap<_, _>>();
    let mut result = Vec::with_capacity(modules.len());
    for module in modules {
        let mut bindings = Vec::new();
        for function in &module.module.functions {
            for pattern in &function.phrases {
                bindings.push(PhraseBinding {
                    module: module.module.name.clone(),
                    function: function.name.clone(),
                    vis: function.vis,
                    pattern: pattern.clone(),
                    params: function.params.iter().map(|(_, ty)| ty.clone()).collect(),
                    type_params: function.type_params.clone(),
                });
            }
        }
        for import in &module.module.imports {
            let Some(index) = by_name.get(&import.module) else {
                continue;
            };
            for function in &modules[*index].module.functions {
                if function.vis != Visibility::Public {
                    continue;
                }
                for pattern in &function.phrases {
                    bindings.push(PhraseBinding {
                        module: modules[*index].module.name.clone(),
                        function: function.name.clone(),
                        vis: Visibility::Public,
                        pattern: pattern.clone(),
                        params: function.params.iter().map(|(_, ty)| ty.clone()).collect(),
                        type_params: function.type_params.clone(),
                    });
                }
            }
        }
        result.push(bindings);
    }
    result
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
enum Target {
    Local(String),
    Global(String),
    Memory(AstExpr),
}

impl Target {
    fn read(&self) -> AstExpr {
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

struct Lowerer<'a> {
    parser: &'a crate::LinkGrammarParser,
    module: String,
    globals: HashMap<String, AstType>,
    types: HashSet<String>,
    module_names: Vec<String>,
    phrases: Vec<PhraseBinding>,
    scopes: Vec<Scope>,
    trace: Vec<SentenceTrace>,
    errors: Vec<SemanticError>,
    temp_counter: usize,
    loop_depth: usize,
}

impl<'a> Lowerer<'a> {
    fn new(
        parser: &'a crate::LinkGrammarParser,
        module: &str,
        globals: HashMap<String, AstType>,
        types: HashSet<String>,
        module_names: Vec<String>,
        phrases: Vec<PhraseBinding>,
    ) -> Self {
        Self {
            parser,
            module: module.to_string(),
            globals,
            types,
            module_names,
            phrases,
            scopes: vec![Scope::new()],
            trace: Vec::new(),
            errors: Vec::new(),
            temp_counter: 0,
            loop_depth: 0,
        }
    }

    fn span(&self, span: Span) -> tc_typeck::Span {
        type_span(&self.module, span)
    }

    fn error(
        &self,
        span: Span,
        kind: SemanticErrorKind,
        message: impl Into<String>,
    ) -> SemanticError {
        semantic_error(&self.module, span, kind, message)
    }

    fn push_scope(&mut self) {
        self.scopes.push(Scope::new());
    }

    fn pop_scope(&mut self) {
        if self.scopes.len() > 1 {
            self.scopes.pop();
        }
    }

    fn declare(&mut self, name: &str, ty: AstType) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.vars.insert(name.to_string(), ty);
        }
    }

    fn lookup(&self, name: &str) -> Option<&AstType> {
        self.scopes
            .iter()
            .rev()
            .find_map(|scope| scope.vars.get(name))
            .or_else(|| self.globals.get(name))
    }

    fn current_scope_contains(&self, name: &str) -> bool {
        self.scopes
            .last()
            .is_some_and(|scope| scope.vars.contains_key(name))
    }

    fn expr_env(&self) -> ExprEnv {
        let mut locals = HashMap::new();
        for scope in &self.scopes {
            for (name, ty) in &scope.vars {
                locals.insert(name.clone(), ty.clone());
            }
        }
        ExprEnv {
            locals,
            globals: self.globals.clone(),
            modules: self.module_names.clone(),
        }
    }

    fn parse_expr(&self, text: &str, span: Span) -> Result<AstExpr, SemanticError> {
        parse_expression(text, &self.expr_env())
            .map_err(|message| self.error(span, SemanticErrorKind::Lowering, message))
    }

    fn phrase_expression(
        &self,
        phrase: &PhraseMatch,
        span: Span,
    ) -> Result<AstExpr, SemanticError> {
        let mut args = Vec::with_capacity(phrase.args.len());
        for argument in &phrase.args {
            args.push(self.parse_expr(argument, span)?);
        }
        let module = (phrase.module != self.module).then(|| phrase.module.clone());
        Ok(AstExpr::Call {
            module,
            name: phrase.function.clone(),
            type_args: Vec::new(),
            args,
        })
    }

    fn parse_value(&self, text: &str, span: Span) -> Result<AstExpr, SemanticError> {
        if let Some(phrase) = self.match_phrase_typed(text, span)? {
            return self.phrase_expression(&phrase, span);
        }
        self.parse_expr(text, span)
    }

    fn type_compatibility(
        &self,
        param: &AstType,
        actual: &AstType,
        type_params: &[String],
        expression: &AstExpr,
    ) -> i8 {
        if let AstType::Named { name, .. } = param {
            if type_params.iter().any(|generic| generic == name) {
                return 2;
            }
        }
        if param == actual {
            return 2;
        }
        match (expression, param) {
            (
                AstExpr::ConstInt(_, None),
                AstType::I8 | AstType::I16 | AstType::I32 | AstType::I64,
            )
            | (
                AstExpr::ConstInt(_, None),
                AstType::U8 | AstType::U16 | AstType::U32 | AstType::U64,
            )
            | (
                AstExpr::ConstUint(_, None),
                AstType::I8 | AstType::I16 | AstType::I32 | AstType::I64,
            )
            | (
                AstExpr::ConstUint(_, None),
                AstType::U8 | AstType::U16 | AstType::U32 | AstType::U64,
            ) => 1,
            (AstExpr::ConstFloat(_, None), AstType::F32 | AstType::F64) => 1,
            (AstExpr::BytesLit(_), AstType::Ptr(inner)) if **inner == AstType::U8 => 1,
            _ => 0,
        }
    }

    fn match_phrase_typed(
        &self,
        text: &str,
        span: Span,
    ) -> Result<Option<PhraseMatch>, SemanticError> {
        let candidates = match_all(text, &self.phrases);
        let Some(max_score) = candidates.iter().map(|candidate| candidate.score).max() else {
            return Ok(None);
        };
        let candidates = candidates
            .into_iter()
            .filter(|candidate| candidate.score == max_score)
            .collect::<Vec<_>>();
        if candidates.len() == 1 {
            return Ok(candidates.into_iter().next());
        }
        let env = self.expr_env();
        let mut scored = candidates
            .into_iter()
            .map(|candidate| {
                let mut compatibility = 0_i8;
                let mut usable = true;
                for (argument, param) in candidate.args.iter().zip(candidate.params.iter()) {
                    let Ok(expression) = parse_expression(argument, &env) else {
                        usable = false;
                        break;
                    };
                    let actual = self.shallow_type(&expression);
                    let score = self.type_compatibility(
                        param,
                        &actual,
                        &candidate.type_params,
                        &expression,
                    );
                    if score == 0 {
                        usable = false;
                        break;
                    }
                    compatibility += score;
                }
                (usable, compatibility, candidate)
            })
            .collect::<Vec<_>>();
        scored.retain(|(usable, _, _)| *usable);
        let Some(best_score) = scored.iter().map(|(_, score, _)| *score).max() else {
            return Ok(None);
        };
        let mut best = scored
            .into_iter()
            .filter(|(_, score, _)| *score == best_score)
            .collect::<Vec<_>>();
        best.sort_by(|left, right| {
            left.2
                .module
                .cmp(&right.2.module)
                .then_with(|| left.2.function.cmp(&right.2.function))
        });
        if best.len() > 1 {
            let names = best
                .iter()
                .map(|(_, _, candidate)| format!("{}::{}", candidate.module, candidate.function))
                .collect::<Vec<_>>()
                .join(", ");
            return Err(self.error(
                span,
                SemanticErrorKind::Lowering,
                format!("ambiguous Phrase dispatch (candidates: {names})"),
            ));
        }
        Ok(best.into_iter().next().map(|(_, _, candidate)| candidate))
    }

    fn shallow_type(&self, expr: &AstExpr) -> AstType {
        match expr {
            AstExpr::ConstInt(_, Some(ty)) => ty.clone(),
            AstExpr::ConstInt(..) => AstType::I64,
            AstExpr::ConstUint(_, Some(ty)) => ty.clone(),
            AstExpr::ConstUint(..) => AstType::U64,
            AstExpr::ConstFloat(_, Some(ty)) => ty.clone(),
            AstExpr::ConstFloat(..) => AstType::F64,
            AstExpr::ConstBool(_) => AstType::Bool,
            AstExpr::ConstNull(ty) => ty.clone(),
            AstExpr::BytesLit(_) => AstType::ptr(AstType::U8),
            AstExpr::BytesLen(_) => AstType::U64,
            AstExpr::Zero(ty) => ty.clone(),
            AstExpr::Var(name) => self.lookup(name).cloned().unwrap_or(AstType::I64),
            AstExpr::Binary { op, lhs, .. } => {
                if matches!(
                    op,
                    tc_ir::BinOp::Eq
                        | tc_ir::BinOp::Ne
                        | tc_ir::BinOp::Lt
                        | tc_ir::BinOp::Le
                        | tc_ir::BinOp::Gt
                        | tc_ir::BinOp::Ge
                        | tc_ir::BinOp::LogAnd
                        | tc_ir::BinOp::LogOr
                ) {
                    AstType::Bool
                } else {
                    self.shallow_type(lhs)
                }
            }
            AstExpr::Unary { op, val } => {
                if matches!(op, tc_ir::UnOp::LogNot) {
                    AstType::Bool
                } else {
                    self.shallow_type(val)
                }
            }
            AstExpr::Cast { to, .. } => to.clone(),
            AstExpr::SizeOf(_) | AstExpr::AlignOf(_) => AstType::U64,
            AstExpr::Load(pointer) => match self.shallow_type(pointer) {
                AstType::Ptr(inner) => *inner,
                _ => AstType::I64,
            },
            _ => AstType::I64,
        }
    }

    fn trace_sentence(
        &mut self,
        sentence: &RawSentence,
        clause: &ClauseTree,
        phrase: Option<&PhraseMatch>,
    ) -> IntentVerdict {
        let verdict = classify(&sentence.text, clause, phrase);
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

    fn lower_items(
        &mut self,
        items: &[RawItem],
        depth: usize,
    ) -> Result<Vec<AstStmt>, SemanticError> {
        if depth > MAX_DEPTH {
            return Err(semantic_error(
                &self.module,
                Span::new(1, 1),
                SemanticErrorKind::Lowering,
                "statement nesting exceeded the 32-level guard",
            ));
        }
        let mut statements = Vec::new();
        let mut index = 0;
        while index < items.len() {
            match &items[index] {
                RawItem::Sentence(sentence) => {
                    if sentence.text.to_ascii_lowercase().starts_with("phrase:") {
                        index += 1;
                        continue;
                    }
                    let clause = analyze_clause(self.parser, sentence);
                    let phrase = self.match_phrase_typed(&sentence.text, sentence.span)?;
                    let verdict = self.trace_sentence(sentence, &clause, phrase.as_ref());
                    match verdict {
                        IntentVerdict::Exec => {
                            if let Some(phrase) = phrase {
                                statements.push(self.lower_phrase_call(&phrase, sentence.span)?);
                            } else {
                                statements.extend(self.lower_stmt(
                                    sentence,
                                    None,
                                    None,
                                    depth + 1,
                                )?);
                            }
                        }
                        IntentVerdict::CommentOmitted { .. } => {}
                        IntentVerdict::Unsupported { .. } => {}
                    }
                    index += 1;
                }
                RawItem::Block(block) => {
                    let Some(header) = &block.header else {
                        index += 1;
                        continue;
                    };
                    if header.text.to_ascii_lowercase().starts_with("phrase:") {
                        index += 1;
                        continue;
                    }
                    let clause = analyze_clause(self.parser, header);
                    let phrase = self.match_phrase_typed(&header.text, header.span)?;
                    let verdict = self.trace_sentence(header, &clause, phrase.as_ref());
                    if !verdict.is_exec() {
                        index += 1;
                        continue;
                    }
                    let lower = header.text.to_ascii_lowercase();
                    let mut else_items = None;
                    if starts_with_word(&lower, "if") {
                        if let Some(RawItem::Block(next)) = items.get(index + 1) {
                            if let Some(next_header) = &next.header {
                                let next_lower = next_header.text.to_ascii_lowercase();
                                if starts_with_word(&next_lower, "else")
                                    || starts_with_word(&next_lower, "otherwise")
                                {
                                    let next_clause = analyze_clause(self.parser, next_header);
                                    let _ = self.trace_sentence(next_header, &next_clause, None);
                                    else_items = Some(next.items.as_slice());
                                    index += 1;
                                }
                            }
                        }
                    }
                    let is_control = ["if", "unless", "while", "repeat", "loop"]
                        .iter()
                        .any(|keyword| starts_with_word(&lower, keyword));
                    if !is_control {
                        return Err(self.error(
                            header.span,
                            SemanticErrorKind::Lowering,
                            format!("unsupported block header `{}`", header.text),
                        ));
                    }
                    statements.extend(self.lower_stmt(
                        header,
                        Some(&block.items),
                        else_items,
                        depth + 1,
                    )?);
                    index += 1;
                }
            }
        }
        Ok(statements)
    }

    fn next_temp(&mut self, label: &str) -> String {
        self.temp_counter += 1;
        format!("__{label}_{}", self.temp_counter)
    }

    fn lower_repeat_count(&self, text: &str, span: Span) -> Result<AstExpr, SemanticError> {
        let rest = strip_leading_word(text, "repeat");
        let count_text = if let Some((start, _)) = find_top_level_word(rest, "times") {
            rest[..start].trim()
        } else {
            rest
        };
        self.parse_expr(count_text, span)
    }

    fn lower_condition(&self, text: &str, span: Span) -> Result<AstExpr, SemanticError> {
        let (condition, where_clause) = split_where(text);
        if where_clause.is_some() {
            return Err(self.error(
                span,
                SemanticErrorKind::Lowering,
                "`where` bindings on a control header must be lowered by the enclosing statement",
            ));
        }
        let condition = condition
            .strip_prefix(',')
            .map_or(condition.as_str(), str::trim);
        self.parse_expr(condition, span)
    }

    fn lower_phrase_call(
        &self,
        phrase: &PhraseMatch,
        span: Span,
    ) -> Result<AstStmt, SemanticError> {
        Ok(AstStmt::Expr(
            self.phrase_expression(phrase, span)?,
            self.span(span),
        ))
    }

    fn lower_stmt(
        &mut self,
        sentence: &RawSentence,
        then_items: Option<&[RawItem]>,
        else_items: Option<&[RawItem]>,
        depth: usize,
    ) -> Result<Vec<AstStmt>, SemanticError> {
        if depth > MAX_DEPTH {
            return Err(self.error(
                sentence.span,
                SemanticErrorKind::Lowering,
                "statement nesting exceeded the 32-level guard",
            ));
        }
        let (core, where_clause) = split_where(&sentence.text);
        if let Some(bindings) = where_clause {
            let mut block = self.lower_where_bindings(&bindings, sentence.span)?;
            let inner = self.lower_core(&core, sentence.span, then_items, else_items, depth + 1)?;
            block.extend(inner);
            return Ok(vec![AstStmt::Block(block, self.span(sentence.span))]);
        }
        self.lower_core(&core, sentence.span, then_items, else_items, depth + 1)
    }

    fn lower_where_bindings(
        &mut self,
        bindings: &str,
        span: Span,
    ) -> Result<Vec<AstStmt>, SemanticError> {
        let mut statements = Vec::new();
        let mut pieces = Vec::new();
        for comma in split_top_level(bindings, ",") {
            pieces.extend(split_top_level(&comma, "and"));
        }
        for piece in pieces {
            let binding = piece.trim();
            if binding.is_empty() {
                continue;
            }
            let Some((start, end)) =
                find_top_level_word(binding, "is").or_else(|| find_top_level_symbol(binding, "="))
            else {
                return Err(self.error(
                    span,
                    SemanticErrorKind::Lowering,
                    format!("`where` binding `{binding}` must use `<name> is <expression>`"),
                ));
            };
            let name = lower_name(binding[..start].trim());
            let value_text = binding[end..].trim();
            let value = self.parse_expr(value_text, span)?;
            let ty = self.shallow_type(&value);
            self.declare(&name, ty);
            statements.push(AstStmt::Let {
                name,
                ty: None,
                init: value,
                span: self.span(span),
            });
        }
        Ok(statements)
    }

    fn lower_core(
        &mut self,
        text: &str,
        span: Span,
        then_items: Option<&[RawItem]>,
        else_items: Option<&[RawItem]>,
        depth: usize,
    ) -> Result<Vec<AstStmt>, SemanticError> {
        let trimmed = text.trim().trim_end_matches(['.', ',']).trim();
        if trimmed.is_empty() {
            return Ok(Vec::new());
        }
        let lower = trimmed.to_ascii_lowercase();
        if lower == "end" {
            return Ok(Vec::new());
        }
        if starts_with_word(&lower, "return") {
            let rest = strip_leading_word(trimmed, "return");
            let rest = rest.trim();
            let value = if rest.is_empty() {
                None
            } else {
                Some(self.parse_value(rest, span)?)
            };
            return Ok(vec![AstStmt::Return(value, self.span(span))]);
        }
        if starts_with_word(&lower, "break") {
            return Ok(vec![AstStmt::Break(self.span(span))]);
        }
        if starts_with_word(&lower, "continue") {
            return Ok(vec![AstStmt::Continue(self.span(span))]);
        }
        if starts_with_word(&lower, "trap") {
            return Ok(vec![AstStmt::Trap(self.span(span))]);
        }
        if starts_with_word(&lower, "define") {
            if let Some(statements) = self.try_local_declaration(trimmed, span) {
                return Ok(statements);
            }
        }
        if starts_with_word(&lower, "syscall") {
            if let Some(statement) = self.try_syscall_with(trimmed, span)? {
                return Ok(vec![statement]);
            }
        }
        if starts_with_word(&lower, "copy") || starts_with_word(&lower, "memcopy") {
            if let Some(statement) = self.try_memcopy(trimmed, span)? {
                return Ok(vec![statement]);
            }
        }
        if starts_with_word(&lower, "if") {
            if let Some(items) = then_items {
                let condition = self.lower_condition(strip_leading_word(trimmed, "if"), span)?;
                self.push_scope();
                let then_body = self.lower_items(items, depth + 1);
                self.pop_scope();
                let then_body = then_body?;
                let else_body = if let Some(items) = else_items {
                    self.push_scope();
                    let body = self.lower_items(items, depth + 1);
                    self.pop_scope();
                    body?
                } else {
                    Vec::new()
                };
                return Ok(vec![AstStmt::If {
                    cond: condition,
                    then_body,
                    else_body,
                    span: self.span(span),
                }]);
            }
            return self.lower_inline_if(trimmed, span, depth);
        }
        if starts_with_word(&lower, "unless") {
            if let Some(items) = then_items {
                let condition =
                    self.lower_condition(strip_leading_word(trimmed, "unless"), span)?;
                self.push_scope();
                let body = self.lower_items(items, depth + 1);
                self.pop_scope();
                return Ok(vec![AstStmt::If {
                    cond: AstExpr::Unary {
                        op: tc_ir::UnOp::LogNot,
                        val: Box::new(condition),
                    },
                    then_body: body?,
                    else_body: Vec::new(),
                    span: self.span(span),
                }]);
            }
            let (condition, actions) =
                self.split_condition_actions(strip_leading_word(trimmed, "unless"), span)?;
            let cond = AstExpr::Unary {
                op: tc_ir::UnOp::LogNot,
                val: Box::new(self.parse_expr(&condition, span)?),
            };
            self.push_scope();
            let body = self.lower_actions(&actions, span, depth);
            self.pop_scope();
            return Ok(vec![AstStmt::If {
                cond,
                then_body: body?,
                else_body: Vec::new(),
                span: self.span(span),
            }]);
        }
        if starts_with_word(&lower, "while") {
            let (cond, body) = if let Some(items) = then_items {
                let cond = self.lower_condition(strip_leading_word(trimmed, "while"), span)?;
                self.loop_depth += 1;
                self.push_scope();
                let body = self.lower_items(items, depth + 1);
                self.pop_scope();
                self.loop_depth -= 1;
                (cond, body?)
            } else {
                let (condition, actions) =
                    self.split_condition_actions(strip_leading_word(trimmed, "while"), span)?;
                let cond = self.parse_expr(&condition, span)?;
                self.loop_depth += 1;
                self.push_scope();
                let body = self.lower_actions(&actions, span, depth);
                self.pop_scope();
                self.loop_depth -= 1;
                (cond, body?)
            };
            let mut loop_body = vec![AstStmt::If {
                cond: AstExpr::Unary {
                    op: tc_ir::UnOp::LogNot,
                    val: Box::new(cond),
                },
                then_body: vec![AstStmt::Break(self.span(span))],
                else_body: Vec::new(),
                span: self.span(span),
            }];
            loop_body.extend(body);
            return Ok(vec![AstStmt::Loop {
                body: loop_body,
                span: self.span(span),
            }]);
        }
        if starts_with_word(&lower, "repeat") {
            let inline = if then_items.is_none() {
                let rest = strip_leading_word(trimmed, "repeat");
                if let Some((start, end)) = find_top_level_word(rest, "times") {
                    Some((
                        rest[..start].trim().to_string(),
                        rest[end..].trim().to_string(),
                    ))
                } else {
                    Some(self.split_condition_actions(rest, span)?)
                }
            } else {
                None
            };
            let count = if let Some((count_text, _)) = &inline {
                self.parse_expr(count_text, span)?
            } else {
                self.lower_repeat_count(trimmed, span)?
            };
            let counter = self.next_temp("repeat");
            let counter_ty = self.shallow_type(&count);
            self.declare(&counter, counter_ty);
            let mut loop_body = vec![AstStmt::If {
                cond: AstExpr::Unary {
                    op: tc_ir::UnOp::LogNot,
                    val: Box::new(AstExpr::Binary {
                        op: tc_ir::BinOp::Lt,
                        lhs: Box::new(AstExpr::Var(counter.clone())),
                        rhs: Box::new(count.clone()),
                    }),
                },
                then_body: vec![AstStmt::Break(self.span(span))],
                else_body: Vec::new(),
                span: self.span(span),
            }];
            self.loop_depth += 1;
            self.push_scope();
            let body = if let Some(items) = then_items {
                self.lower_items(items, depth + 1)
            } else {
                self.lower_actions(&inline.expect("inline repeat").1, span, depth)
            };
            self.pop_scope();
            self.loop_depth -= 1;
            loop_body.extend(body?);
            loop_body.push(AstStmt::Assign {
                name: counter.clone(),
                val: AstExpr::Binary {
                    op: tc_ir::BinOp::Add,
                    lhs: Box::new(AstExpr::Var(counter.clone())),
                    rhs: Box::new(AstExpr::ConstInt(1, None)),
                },
                span: self.span(span),
            });
            return Ok(vec![AstStmt::Block(
                vec![
                    AstStmt::Let {
                        name: counter,
                        ty: None,
                        init: zero_like(&count),
                        span: self.span(span),
                    },
                    AstStmt::Loop {
                        body: loop_body,
                        span: self.span(span),
                    },
                ],
                self.span(span),
            )]);
        }
        if starts_with_word(&lower, "loop") {
            let body = if let Some(items) = then_items {
                self.loop_depth += 1;
                self.push_scope();
                let body = self.lower_items(items, depth + 1);
                self.pop_scope();
                self.loop_depth -= 1;
                body?
            } else {
                Vec::new()
            };
            return Ok(vec![AstStmt::Loop {
                body,
                span: self.span(span),
            }]);
        }
        if starts_with_word(&lower, "else") || starts_with_word(&lower, "otherwise") {
            return Err(self.error(
                span,
                SemanticErrorKind::Lowering,
                "`Else`/`Otherwise` must follow an `If` block",
            ));
        }
        if let Some(statement) = self.try_assignment(trimmed, span)? {
            return Ok(statement);
        }
        if let Some(statement) = self.try_computational(trimmed, span)? {
            return Ok(statement);
        }
        let value = self.parse_value(trimmed, span)?;
        Ok(vec![AstStmt::Expr(value, self.span(span))])
    }

    fn try_memcopy(&self, text: &str, span: Span) -> Result<Option<AstStmt>, SemanticError> {
        let rest = if starts_with_word(text, "memcopy") {
            strip_leading_word(text, "memcopy")
        } else {
            strip_leading_word(text, "copy")
        };
        let Some((bytes_start, bytes_end)) = find_top_level_word(rest, "bytes") else {
            return Ok(None);
        };
        let Some((_, from_end)) = find_top_level_word(&rest[bytes_end..], "from") else {
            return Ok(None);
        };
        let from_end = bytes_end + from_end;
        let Some((to_start, to_end)) = find_top_level_word(&rest[from_end..], "to")
            .or_else(|| find_top_level_word(&rest[from_end..], "into"))
        else {
            return Ok(None);
        };
        let to_start = from_end + to_start;
        let to_end = from_end + to_end;
        let size_text = rest[..bytes_start].trim();
        let src_text = rest[from_end..to_start].trim();
        let dst_text = rest[to_end..].trim();
        if size_text.is_empty() || src_text.is_empty() || dst_text.is_empty() {
            return Ok(None);
        }
        Ok(Some(AstStmt::MemCopy {
            dst: self.parse_value(dst_text, span)?,
            src: self.parse_value(src_text, span)?,
            size: self.parse_value(size_text, span)?,
            span: self.span(span),
        }))
    }

    fn try_syscall_with(&self, text: &str, span: Span) -> Result<Option<AstStmt>, SemanticError> {
        let rest = strip_leading_word(text, "syscall");
        let Some((start, end)) = find_top_level_word(rest, "with") else {
            return Ok(None);
        };
        let num_text = rest[..start].trim();
        let args_text = rest[end..].trim();
        let mut args = Vec::new();
        for argument in split_top_level(args_text, ",") {
            args.push(self.parse_expr(&argument, span)?);
        }
        Ok(Some(AstStmt::Expr(
            AstExpr::Syscall {
                num: Box::new(self.parse_expr(num_text, span)?),
                args,
            },
            self.span(span),
        )))
    }

    fn split_condition_actions(
        &self,
        text: &str,
        span: Span,
    ) -> Result<(String, String), SemanticError> {
        let text = text.trim();
        let pieces = split_top_level(text, ",");
        if pieces.len() >= 2 {
            return Ok((pieces[0].clone(), pieces[1..].join(", ")));
        }
        // No comma: find the longest prefix that parses as a condition and
        // leaves a non-empty action.
        let tokens = tokenize(text);
        for split in (1..tokens.len()).rev() {
            let boundary = tokens[split].start;
            let condition = text[..boundary].trim();
            let action = text[boundary..].trim();
            if condition.is_empty() || action.is_empty() {
                continue;
            }
            if self.parse_expr(condition, span).is_ok() {
                return Ok((condition.to_string(), action.to_string()));
            }
        }
        Err(self.error(
            span,
            SemanticErrorKind::Lowering,
            format!("cannot separate condition from action in `{text}`"),
        ))
    }

    fn lower_inline_if(
        &mut self,
        text: &str,
        span: Span,
        depth: usize,
    ) -> Result<Vec<AstStmt>, SemanticError> {
        let rest = strip_leading_word(text, "if");
        let (condition_part, mut remainder) = self.split_condition_actions(rest, span)?;
        let mut else_actions = None;
        for marker in ["otherwise", "else"] {
            if let Some((start, end)) = find_top_level_word(&remainder, marker) {
                let before = remainder[..start].trim().to_string();
                let after = remainder[end..].trim().to_string();
                if !after.is_empty() && !before.is_empty() {
                    remainder = before;
                    else_actions = Some(after);
                    break;
                }
            }
        }
        let cond = self.parse_expr(&condition_part, span)?;
        self.push_scope();
        let then_body = self.lower_actions(&remainder, span, depth)?;
        self.pop_scope();
        let else_body = if let Some(actions) = else_actions {
            self.push_scope();
            let body = self.lower_actions(&actions, span, depth);
            self.pop_scope();
            body?
        } else {
            Vec::new()
        };
        Ok(vec![AstStmt::If {
            cond,
            then_body,
            else_body,
            span: self.span(span),
        }])
    }

    fn lower_actions(
        &mut self,
        text: &str,
        span: Span,
        depth: usize,
    ) -> Result<Vec<AstStmt>, SemanticError> {
        let mut statements = Vec::new();
        for action in split_actions(text) {
            let sentence = RawSentence {
                text: action,
                raw: String::new(),
                span,
                fillers: Vec::new(),
            };
            statements.extend(self.lower_stmt(&sentence, None, None, depth + 1)?);
        }
        Ok(statements)
    }

    fn declaration_type(&self, text: &str) -> Option<AstType> {
        let trimmed = text.trim();
        let stripped = trimmed
            .strip_prefix("a ")
            .or_else(|| trimmed.strip_prefix("an "))
            .unwrap_or(trimmed);
        if let Ok(ty) = parse_type_text(stripped) {
            if let AstType::Named { module, name, .. } = &ty {
                if module.is_some() || self.types.contains(name) {
                    return Some(ty);
                }
            }
        }
        let words = tokenize(trimmed)
            .into_iter()
            .filter(tc_syntax::Token::is_word)
            .map(|token| token.lower)
            .collect::<Vec<_>>();
        let name = match words.as_slice() {
            [name] => name,
            [article, name] if article == "a" || article == "an" => name,
            _ => return None,
        };
        self.types.contains(name).then(|| AstType::Named {
            module: None,
            name: name.clone(),
            type_args: Vec::new(),
        })
    }

    fn declare_local(&mut self, name: &str, ty: AstType, span: Span) -> AstStmt {
        let init = match &ty {
            AstType::Ptr(_) => AstExpr::ConstNull(ty.clone()),
            AstType::Named { .. } | AstType::Array(..) => AstExpr::Zero(ty.clone()),
            AstType::F32 | AstType::F64 => AstExpr::ConstFloat(0.0, Some(ty.clone())),
            AstType::Bool => AstExpr::ConstBool(false),
            AstType::Void => AstExpr::ConstInt(0, None),
            AstType::U8 | AstType::U16 | AstType::U32 | AstType::U64 => {
                AstExpr::ConstUint(0, Some(ty.clone()))
            }
            _ => AstExpr::ConstInt(0, Some(ty.clone())),
        };
        self.declare(name, ty.clone());
        AstStmt::Let {
            name: name.to_string(),
            ty: Some(ty),
            init,
            span: self.span(span),
        }
    }

    fn try_local_declaration(&mut self, text: &str, span: Span) -> Option<Vec<AstStmt>> {
        let tokens = tokenize(text);
        let mut words = tokens.iter().filter(|token| token.is_word());
        if !words.next().is_some_and(|token| token.is("define")) {
            return None;
        }
        let mut next = words.next()?;
        if next.is("local") {
            next = words.next()?;
        }
        let name = lower_name(&next.text);
        if words.next().is_some_and(|token| token.is("as")) {
            let type_start = tokens
                .iter()
                .position(|token| token.is("as"))
                .and_then(|index| tokens.get(index + 1))
                .map_or(text.len(), |token| token.start);
            let ty_text = text[type_start..].trim().trim_end_matches('.');
            if let Ok(ty) = parse_type_text(ty_text) {
                if self.lookup(&name).is_some() {
                    return None;
                }
                return Some(vec![self.declare_local(&name, ty, span)]);
            }
        }
        None
    }

    fn try_assignment(
        &mut self,
        text: &str,
        span: Span,
    ) -> Result<Option<Vec<AstStmt>>, SemanticError> {
        for (symbol, operator) in [
            ("+=", tc_ir::BinOp::Add),
            ("-=", tc_ir::BinOp::Sub),
            ("*=", tc_ir::BinOp::Mul),
            ("/=", tc_ir::BinOp::Div),
        ] {
            if let Some((start, end)) = find_top_level_symbol(text, symbol) {
                let target_text = text[..start].trim();
                let value_text = text[end..].trim();
                let value = self.parse_expr(value_text, span)?;
                return Ok(Some(self.write_arithmetic(
                    target_text,
                    operator,
                    value,
                    span,
                )?));
            }
        }
        if starts_with_word(text, "let") {
            let rest = strip_leading_word(text, "let");
            if let Some((start, end)) = find_top_level_word(rest, "be") {
                let target = rest[..start].trim();
                let value_text = rest[end..].trim();
                let value = self.parse_value(value_text, span)?;
                return Ok(Some(self.assign_text(target, value, span, true)?));
            }
        }
        for keyword in ["is", "becomes", "equals"] {
            if let Some((start, end)) = find_top_level_word(text, keyword) {
                let target = text[..start].trim();
                let value_text = text[end..].trim();
                if target.is_empty()
                    || !is_lvalue_text(target)
                    || target.chars().any(char::is_whitespace)
                {
                    continue;
                }
                if is_simple_name(target) {
                    let name = lower_name(target);
                    if let Some(ty) = self.declaration_type(value_text) {
                        if self.lookup(&name).is_none() {
                            return Ok(Some(vec![self.declare_local(&name, ty, span)]));
                        }
                    }
                }
                let Ok(value) = self.parse_value(value_text, span) else {
                    continue;
                };
                return Ok(Some(self.assign_text(target, value, span, true)?));
            }
        }
        if let Some((start, end)) = find_top_level_symbol(text, "=") {
            let target = text[..start].trim();
            let value_text = text[end..].trim();
            if is_lvalue_text(target) && !value_text.is_empty() {
                let value = self.parse_value(value_text, span)?;
                return Ok(Some(self.assign_text(target, value, span, true)?));
            }
        }
        Ok(None)
    }

    fn assign_text(
        &mut self,
        target_text: &str,
        value: AstExpr,
        span: Span,
        allow_declare: bool,
    ) -> Result<Vec<AstStmt>, SemanticError> {
        if is_simple_name(target_text) {
            let name = lower_name(target_text);
            if self.current_scope_contains(&name) {
                return Ok(vec![self.write_target(&Target::Local(name), value, span)]);
            }
            if self.globals.contains_key(&name)
                && !self
                    .scopes
                    .iter()
                    .any(|scope| scope.vars.contains_key(&name))
            {
                return Ok(vec![self.write_target(&Target::Global(name), value, span)]);
            }
            if let Some(outer) = self.lookup(&name).cloned() {
                let value_ty = self.shallow_type(&value);
                if outer == value_ty {
                    let target = if self.globals.contains_key(&name)
                        && !self
                            .scopes
                            .iter()
                            .any(|scope| scope.vars.contains_key(&name))
                    {
                        Target::Global(name)
                    } else {
                        Target::Local(name)
                    };
                    return Ok(vec![self.write_target(&target, value, span)]);
                }
                // A type-changing assignment in a nested block is a shadow,
                // not a mutation of the outer binding.
                self.declare(&name, value_ty);
                return Ok(vec![AstStmt::Let {
                    name,
                    ty: None,
                    init: value,
                    span: self.span(span),
                }]);
            }
            if allow_declare {
                let ty = self.shallow_type(&value);
                self.declare(&name, ty);
                return Ok(vec![AstStmt::Let {
                    name,
                    ty: None,
                    init: value,
                    span: self.span(span),
                }]);
            }
            return Err(self.error(
                span,
                SemanticErrorKind::Lowering,
                format!("assignment target `{name}` is not declared"),
            ));
        }
        let address = parse_address(target_text, &self.expr_env())
            .map_err(|message| self.error(span, SemanticErrorKind::Lowering, message))?;
        Ok(vec![AstStmt::Store {
            ptr: address,
            val: value,
            span: self.span(span),
        }])
    }

    fn write_arithmetic(
        &mut self,
        target_text: &str,
        operator: tc_ir::BinOp,
        value: AstExpr,
        span: Span,
    ) -> Result<Vec<AstStmt>, SemanticError> {
        let (prelude, target) = self.resolve_target(target_text, span)?;
        let current = target.read();
        let combined = AstExpr::Binary {
            op: operator,
            lhs: Box::new(current),
            rhs: Box::new(value),
        };
        let mut statements = prelude;
        statements.push(self.write_target(&target, combined, span));
        Ok(statements)
    }

    fn resolve_target(
        &mut self,
        text: &str,
        span: Span,
    ) -> Result<(Vec<AstStmt>, Target), SemanticError> {
        if is_simple_name(text) {
            let name = lower_name(text);
            if self.globals.contains_key(&name) {
                return Ok((Vec::new(), Target::Global(name)));
            }
            if self
                .scopes
                .iter()
                .any(|scope| scope.vars.contains_key(&name))
            {
                return Ok((Vec::new(), Target::Local(name)));
            }
            self.declare(&name, AstType::I64);
            return Ok((
                vec![AstStmt::Let {
                    name: name.clone(),
                    ty: None,
                    init: AstExpr::ConstInt(0, None),
                    span: self.span(span),
                }],
                Target::Local(name),
            ));
        }
        let address = parse_address(text, &self.expr_env())
            .map_err(|message| self.error(span, SemanticErrorKind::Lowering, message))?;
        Ok((Vec::new(), Target::Memory(address)))
    }

    fn write_target(&self, target: &Target, value: AstExpr, span: Span) -> AstStmt {
        match target {
            Target::Local(name) => AstStmt::Assign {
                name: name.clone(),
                val: value,
                span: self.span(span),
            },
            Target::Global(name) => AstStmt::Store {
                ptr: AstExpr::GlobalAddr {
                    module: None,
                    name: name.clone(),
                },
                val: value,
                span: self.span(span),
            },
            Target::Memory(address) => AstStmt::Store {
                ptr: address.clone(),
                val: value,
                span: self.span(span),
            },
        }
    }

    fn try_computational(
        &mut self,
        text: &str,
        span: Span,
    ) -> Result<Option<Vec<AstStmt>>, SemanticError> {
        let tokens = tokenize(text);
        let mut verb_index = None;
        let mut resolution = None;
        for (index, token) in tokens.iter().enumerate() {
            if token.kind != TokenKind::Word
                || matches!(
                    token.lower.as_str(),
                    "am" | "is"
                        | "are"
                        | "was"
                        | "were"
                        | "be"
                        | "been"
                        | "being"
                        | "has"
                        | "have"
                        | "had"
                        | "do"
                        | "does"
                        | "did"
                )
            {
                continue;
            }
            if tokens.get(index + 1).is_some_and(|next| {
                next.is_word()
                    && matches!(
                        next.lower.as_str(),
                        "am" | "is" | "are" | "was" | "were" | "be" | "been" | "being"
                    )
            }) {
                continue;
            }
            let prep = tokens.iter().skip(index + 1).find_map(|candidate| {
                if candidate.kind == TokenKind::Word
                    && matches!(
                        tc_lexicon::lookup_closed_class(&candidate.lower),
                        Some(tc_lexicon::ClosedClass::Preposition)
                    )
                {
                    Some(candidate.lower.clone())
                } else {
                    None
                }
            });
            let resolved = prep
                .as_deref()
                .and_then(|prep| classify_verb(&token.lower, Some(prep)))
                .or_else(|| classify_verb(&token.lower, None));
            if let Some(resolved) = resolved {
                if resolved.predicate.is_some() || resolved.concept.is_some() {
                    verb_index = Some(index);
                    resolution = Some(resolved);
                    break;
                }
            }
        }
        let Some(verb_index) = verb_index else {
            return Ok(None);
        };
        let Some(resolution) = resolution else {
            return Ok(None);
        };
        let clause = analyze_clause(
            self.parser,
            &RawSentence {
                text: text.to_string(),
                raw: String::new(),
                span,
                fillers: Vec::new(),
            },
        );
        let prep_indices = tokens
            .iter()
            .enumerate()
            .filter_map(|(index, token)| {
                (index > verb_index
                    && token.kind == TokenKind::Word
                    && matches!(
                        tc_lexicon::lookup_closed_class(&token.lower),
                        Some(tc_lexicon::ClosedClass::Preposition)
                    ))
                .then_some(index)
            })
            .collect::<Vec<_>>();
        let first_prep = prep_indices.first().copied();
        let prep_word = first_prep.map(|index| tokens[index].lower.clone());
        let role = prep_word.as_deref().and_then(prep_role);
        let mut subject_text = clause
            .subject
            .as_ref()
            .map(|subject| join_tokens(subject))
            .unwrap_or_default();
        if subject_text.is_empty() {
            let before_auxiliary = tokens
                .iter()
                .take_while(|token| {
                    !(token.is_word()
                        && matches!(
                            token.lower.as_str(),
                            "am" | "is" | "are" | "was" | "were" | "be" | "been" | "being"
                        ))
                })
                .filter(|token| token.is_word() || token.kind == TokenKind::Number)
                .cloned()
                .collect::<Vec<_>>();
            subject_text = join_tokens(&before_auxiliary);
        }
        let after_verb_start = tokens
            .get(verb_index + 1)
            .map_or(text.len(), |token| token.start);
        let before_prep_end = first_prep.map_or(text.len(), |index| tokens[index].start);
        let after_prep_start = first_prep
            .and_then(|index| tokens.get(index + 1))
            .map_or(text.len(), |token| token.start);
        let between_verb_prep = text[after_verb_start..before_prep_end].trim();
        let after_prep = text[after_prep_start..].trim();
        let be_auxiliary = verb_index > 0
            && tokens.get(verb_index - 1).is_some_and(|token| {
                token.is_word()
                    && matches!(
                        token.lower.as_str(),
                        "am" | "is" | "are" | "was" | "were" | "be" | "been" | "being"
                    )
            });
        let passive = clause.voice == Voice::Passive || be_auxiliary;
        match resolution.concept {
            Some(SemanticConcept::Double) => {
                let target = if passive {
                    subject_text.as_str()
                } else {
                    after_verb_start_text(&tokens, verb_index, text)
                };
                let value = AstExpr::ConstInt(2, None);
                return Ok(Some(self.write_arithmetic(
                    target,
                    tc_ir::BinOp::Mul,
                    value,
                    span,
                )?));
            }
            Some(SemanticConcept::Halve) => {
                let target = if passive {
                    subject_text.as_str()
                } else {
                    after_verb_start_text(&tokens, verb_index, text)
                };
                let value = AstExpr::ConstInt(2, None);
                return Ok(Some(self.write_arithmetic(
                    target,
                    tc_ir::BinOp::Div,
                    value,
                    span,
                )?));
            }
            Some(SemanticConcept::Negate) => {
                let target = if passive {
                    subject_text.as_str()
                } else {
                    after_verb_start_text(&tokens, verb_index, text)
                };
                let (prelude, target) = self.resolve_target(target, span)?;
                let current = target.read();
                let negated = AstExpr::Unary {
                    op: tc_ir::UnOp::Neg,
                    val: Box::new(current),
                };
                let mut statements = prelude;
                statements.push(self.write_target(&target, negated, span));
                return Ok(Some(statements));
            }
            _ => {}
        }
        let Some(predicate) = resolution.predicate else {
            return Ok(None);
        };
        match predicate {
            VerbPredicate::Assign => {
                if prep_indices.is_empty() {
                    return Ok(None);
                }
                let mut chosen = None;
                for prep_index in &prep_indices {
                    let before = text[after_verb_start..tokens[*prep_index].start].trim();
                    let after = text[tokens[*prep_index].end..].trim();
                    let chosen_role = prep_role(&tokens[*prep_index].lower);
                    if is_lvalue_text(before) {
                        if let Ok(value) = self.parse_expr(after, span) {
                            chosen = Some((before, value, chosen_role));
                            break;
                        }
                    }
                    if is_lvalue_text(after) {
                        if let Ok(value) = self.parse_expr(before, span) {
                            chosen = Some((after, value, chosen_role));
                            break;
                        }
                    }
                }
                let Some((target, value, chosen_role)) = chosen else {
                    return Err(self.error(
                        span,
                        SemanticErrorKind::Lowering,
                        format!(
                            "assignment verb `{}` needs a target and a value",
                            resolution.lemma
                        ),
                    ));
                };
                if chosen_role == Some(tc_lexicon::PrepRole::Location) {
                    let address =
                        parse_store_address(target, &self.expr_env()).map_err(|message| {
                            self.error(span, SemanticErrorKind::Lowering, message)
                        })?;
                    return Ok(Some(vec![AstStmt::Store {
                        ptr: address,
                        val: value,
                        span: self.span(span),
                    }]));
                }
                Ok(Some(self.assign_text(target, value, span, true)?))
            }
            VerbPredicate::TransferAdd => Ok(Some(self.lower_transfer(
                resolution.concept,
                text,
                &tokens,
                verb_index,
                first_prep,
                passive,
                &subject_text,
                role,
                tc_ir::BinOp::Add,
                span,
            )?)),
            VerbPredicate::TransferSub => Ok(Some(self.lower_transfer(
                resolution.concept,
                text,
                &tokens,
                verb_index,
                first_prep,
                passive,
                &subject_text,
                role,
                tc_ir::BinOp::Sub,
                span,
            )?)),
            VerbPredicate::TransferMove => {
                if prep_indices.len() < 2 {
                    return Ok(None);
                }
                let source_index = prep_indices[0];
                let destination_index = prep_indices[1];
                let source = text[tokens[source_index].end..tokens[destination_index].start].trim();
                let destination = text[tokens[destination_index].end..].trim();
                let value_text = if passive && !subject_text.is_empty() {
                    subject_text.as_str()
                } else {
                    between_verb_prep
                };
                let value = self.parse_expr(value_text, span)?;
                let mut statements =
                    self.write_arithmetic(source, tc_ir::BinOp::Sub, value.clone(), span)?;
                statements.extend(self.write_arithmetic(
                    destination,
                    tc_ir::BinOp::Add,
                    value,
                    span,
                )?);
                Ok(Some(statements))
            }
            VerbPredicate::Transform | VerbPredicate::Combine => {
                let Some(operator) = transform_operator(resolution.concept, predicate) else {
                    return Ok(None);
                };
                let (target, value) = if passive {
                    (subject_text.as_str(), after_prep)
                } else {
                    match role {
                        Some(tc_lexicon::PrepRole::Extent) => (between_verb_prep, after_prep),
                        Some(tc_lexicon::PrepRole::Instrument) => (between_verb_prep, after_prep),
                        Some(tc_lexicon::PrepRole::Destination) => (after_prep, between_verb_prep),
                        _ => (between_verb_prep, after_prep),
                    }
                };
                if target.is_empty() {
                    return Ok(None);
                }
                let value = if value.is_empty() {
                    AstExpr::ConstInt(1, None)
                } else {
                    self.parse_expr(value, span)?
                };
                let operator = if predicate == VerbPredicate::Combine {
                    tc_ir::BinOp::Add
                } else {
                    operator
                };
                Ok(Some(self.write_arithmetic(target, operator, value, span)?))
            }
            VerbPredicate::Create
            | VerbPredicate::Destroy
            | VerbPredicate::Emit
            | VerbPredicate::Receive => Ok(None),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn lower_transfer(
        &mut self,
        concept: Option<SemanticConcept>,
        text: &str,
        tokens: &[Token],
        verb_index: usize,
        first_prep: Option<usize>,
        passive: bool,
        subject_text: &str,
        role: Option<tc_lexicon::PrepRole>,
        operator: tc_ir::BinOp,
        span: Span,
    ) -> Result<Vec<AstStmt>, SemanticError> {
        let Some(prep_index) = first_prep else {
            return Ok(Vec::new());
        };
        let after_verb = tokens
            .get(verb_index + 1)
            .map_or(text.len(), |token| token.start);
        let between = text[after_verb..tokens[prep_index].start].trim();
        let after = text[tokens[prep_index].end..].trim();
        let _ = concept;
        let (target, value_text) = if passive {
            match role {
                Some(tc_lexicon::PrepRole::Extent) | Some(tc_lexicon::PrepRole::Instrument) => {
                    (subject_text, after)
                }
                _ => {
                    if after.is_empty() {
                        (subject_text, between)
                    } else {
                        (after, subject_text)
                    }
                }
            }
        } else {
            match role {
                Some(tc_lexicon::PrepRole::Destination) => (after, between),
                Some(tc_lexicon::PrepRole::Source) => (after, between),
                Some(tc_lexicon::PrepRole::Extent) => (between, after),
                Some(tc_lexicon::PrepRole::Instrument) => (between, after),
                _ => {
                    if is_lvalue_text(between) {
                        (between, after)
                    } else {
                        (after, between)
                    }
                }
            }
        };
        if target.is_empty() {
            return Ok(Vec::new());
        }
        let value = if value_text.is_empty() {
            AstExpr::ConstInt(0, None)
        } else {
            self.parse_expr(value_text, span)?
        };
        self.write_arithmetic(target, operator, value, span)
    }
}

fn zero_like(expr: &AstExpr) -> AstExpr {
    match expr {
        AstExpr::ConstFloat(..) => AstExpr::ConstFloat(0.0, None),
        _ => AstExpr::ConstInt(0, None),
    }
}

fn transform_operator(
    concept: Option<SemanticConcept>,
    predicate: VerbPredicate,
) -> Option<tc_ir::BinOp> {
    match concept {
        Some(SemanticConcept::Mul) => Some(tc_ir::BinOp::Mul),
        Some(SemanticConcept::Div) => Some(tc_ir::BinOp::Div),
        Some(SemanticConcept::Mod) => Some(tc_ir::BinOp::Mod),
        _ if predicate == VerbPredicate::Combine => Some(tc_ir::BinOp::Add),
        _ => Some(tc_ir::BinOp::Mul),
    }
}

fn is_simple_name(text: &str) -> bool {
    let tokens = tokenize(text);
    tokens.len() == 1
        && tokens[0].kind == TokenKind::Word
        && tokens[0]
            .text
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '_')
}

fn after_verb_start_text<'t>(tokens: &[Token], verb_index: usize, text: &'t str) -> &'t str {
    text[tokens[verb_index + 1].start..].trim()
}

fn join_tokens(tokens: &[Token]) -> String {
    tokens
        .iter()
        .map(|token| token.text.as_str())
        .collect::<Vec<_>>()
        .join(" ")
}

fn strip_leading_word<'t>(text: &'t str, word: &str) -> &'t str {
    let trimmed = text.trim_start();
    if trimmed.len() >= word.len() && trimmed[..word.len()].eq_ignore_ascii_case(word) {
        trimmed[word.len()..].trim_start()
    } else {
        trimmed
    }
}

fn split_actions(text: &str) -> Vec<String> {
    let mut result = Vec::new();
    for comma_piece in split_top_level(text, ",") {
        let mut remaining = comma_piece.trim().to_string();
        while let Some((start, end)) = find_top_level_word(&remaining, "and") {
            let left = remaining[..start].trim().to_string();
            let right = remaining[end..].trim().to_string();
            if left.is_empty() || right.is_empty() || !starts_action(&right) {
                break;
            }
            result.push(left);
            remaining = right;
        }
        if !remaining.trim().is_empty() {
            result.push(remaining.trim().to_string());
        }
    }
    result
}

fn starts_action(text: &str) -> bool {
    let word = first_word(text);
    if word.is_empty() {
        return false;
    }
    if matches!(
        word.as_str(),
        "let"
            | "return"
            | "break"
            | "continue"
            | "trap"
            | "syscall"
            | "if"
            | "unless"
            | "while"
            | "repeat"
            | "loop"
    ) {
        return true;
    }
    classify_verb(&word, None).is_some()
}

/// Runs the declaration and lowering passes for one module.
pub(crate) fn lower_module(
    module: ModuleJob,
    parser: &crate::LinkGrammarParser,
    module_names: Vec<String>,
    phrases: Vec<PhraseBinding>,
    is_entry: bool,
) -> (AstModule, Vec<SentenceTrace>, Vec<SemanticError>) {
    let mut module_ast = module.module;
    let globals = module_ast
        .globals
        .iter()
        .map(|global| (global.name.clone(), global.ty.clone()))
        .collect::<HashMap<_, _>>();
    let types = module_ast
        .structs
        .iter()
        .map(|value| value.name.clone())
        .chain(module_ast.unions.iter().map(|value| value.name.clone()))
        .collect::<HashSet<_>>();
    let mut lowerer = Lowerer::new(
        parser,
        &module_ast.name,
        globals,
        types,
        module_names,
        phrases,
    );
    for job in module.functions {
        let function = &module_ast.functions[job.index];
        lowerer.scopes = vec![Scope::new()];
        for (name, ty) in &function.params {
            lowerer.declare(name, ty.clone());
        }
        let body = lowerer.lower_items(&job.body, 0).unwrap_or_else(|error| {
            lowerer.errors.push(error);
            Vec::new()
        });
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
            let mut body = lowerer
                .lower_items(&module.executable, 0)
                .unwrap_or_else(|error| {
                    lowerer.errors.push(error);
                    Vec::new()
                });
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
    (module_ast, lowerer.trace, lowerer.errors)
}
