//! Module graph construction, visibility, and symbol resolution.

use std::collections::{HashMap, HashSet};

use crate::ast::{
    AstModule, AstStruct, AstType, AstUnion, Program, Span, TypeError, TypeErrorKind, Visibility,
};
use crate::suggest::{closest_match, humanize};

/// A fully resolved concrete type used during checking and monomorphization.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) enum ConcreteType {
    Void,
    Bool,
    I8,
    I16,
    I32,
    I64,
    U8,
    U16,
    U32,
    U64,
    F32,
    F64,
    Ptr(Box<ConcreteType>),
    Array(Box<ConcreteType>, usize),
    Struct {
        module: String,
        name: String,
        args: Vec<ConcreteType>,
    },
    Union {
        module: String,
        name: String,
        args: Vec<ConcreteType>,
    },
    FnPtr {
        params: Vec<ConcreteType>,
        ret: Box<ConcreteType>,
    },
    Generic(String),
}

impl ConcreteType {
    pub(crate) fn is_signed_int(&self) -> bool {
        matches!(self, Self::I8 | Self::I16 | Self::I32 | Self::I64)
    }

    pub(crate) fn is_unsigned_int(&self) -> bool {
        matches!(self, Self::U8 | Self::U16 | Self::U32 | Self::U64)
    }

    pub(crate) fn is_integer(&self) -> bool {
        self.is_signed_int() || self.is_unsigned_int()
    }

    pub(crate) fn is_float(&self) -> bool {
        matches!(self, Self::F32 | Self::F64)
    }

    pub(crate) fn is_scalar(&self) -> bool {
        matches!(
            self,
            Self::Bool
                | Self::I8
                | Self::I16
                | Self::I32
                | Self::I64
                | Self::U8
                | Self::U16
                | Self::U32
                | Self::U64
                | Self::F32
                | Self::F64
                | Self::Ptr(_)
                | Self::FnPtr { .. }
        )
    }

    pub(crate) fn is_aggregate(&self) -> bool {
        matches!(
            self,
            Self::Array(_, _) | Self::Struct { .. } | Self::Union { .. }
        )
    }

    pub(crate) fn pointee(&self) -> Option<&ConcreteType> {
        if let Self::Ptr(inner) = self {
            Some(inner)
        } else {
            None
        }
    }

    pub(crate) fn contains_generic(&self) -> bool {
        match self {
            Self::Generic(_) => true,
            Self::Ptr(inner) | Self::Array(inner, _) => inner.contains_generic(),
            Self::Struct { args, .. } | Self::Union { args, .. } => {
                args.iter().any(Self::contains_generic)
            }
            Self::FnPtr { params, ret } => {
                params.iter().any(Self::contains_generic) || ret.contains_generic()
            }
            _ => false,
        }
    }

    pub(crate) fn substitute(&self, map: &HashMap<String, ConcreteType>) -> Self {
        match self {
            Self::Generic(name) => map.get(name).cloned().unwrap_or_else(|| self.clone()),
            Self::Ptr(inner) => Self::Ptr(Box::new(inner.substitute(map))),
            Self::Array(inner, count) => Self::Array(Box::new(inner.substitute(map)), *count),
            Self::Struct { module, name, args } => Self::Struct {
                module: module.clone(),
                name: name.clone(),
                args: args.iter().map(|arg| arg.substitute(map)).collect(),
            },
            Self::Union { module, name, args } => Self::Union {
                module: module.clone(),
                name: name.clone(),
                args: args.iter().map(|arg| arg.substitute(map)).collect(),
            },
            Self::FnPtr { params, ret } => Self::FnPtr {
                params: params.iter().map(|param| param.substitute(map)).collect(),
                ret: Box::new(ret.substitute(map)),
            },
            _ => self.clone(),
        }
    }

    pub(crate) fn mangle(&self) -> String {
        match self {
            Self::Void => "void".to_string(),
            Self::Bool => "bool".to_string(),
            Self::I8 => "i8".to_string(),
            Self::I16 => "i16".to_string(),
            Self::I32 => "i32".to_string(),
            Self::I64 => "i64".to_string(),
            Self::U8 => "u8".to_string(),
            Self::U16 => "u16".to_string(),
            Self::U32 => "u32".to_string(),
            Self::U64 => "u64".to_string(),
            Self::F32 => "f32".to_string(),
            Self::F64 => "f64".to_string(),
            Self::Ptr(inner) => format!("ptr_{}", inner.mangle()),
            Self::Array(inner, count) => format!("arr{count}_{}", inner.mangle()),
            Self::Struct { module, name, args } | Self::Union { module, name, args } => {
                let mut result = format!("{}__{}", sanitize(module), sanitize(name));
                for arg in args {
                    result.push_str("__");
                    result.push_str(&arg.mangle());
                }
                result
            }
            Self::FnPtr { params, ret } => {
                let mut result = String::from("fnptr");
                for param in params {
                    result.push_str("__");
                    result.push_str(&param.mangle());
                }
                result.push_str("__ret__");
                result.push_str(&ret.mangle());
                result
            }
            Self::Generic(name) => format!("generic_{}", sanitize(name)),
        }
    }
}

pub(crate) fn sanitize(value: &str) -> String {
    value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '_' {
                ch
            } else {
                '_'
            }
        })
        .collect()
}

/// Reference to one module.
#[derive(Debug)]
pub(crate) struct ResolvedModule<'a> {
    pub ast: &'a AstModule,
    pub index: usize,
    pub imports: Vec<ResolvedImport>,
    pub import_map: HashMap<String, usize>,
    pub imported_modules: Vec<usize>,
    pub structs: HashMap<String, usize>,
    pub unions: HashMap<String, usize>,
    pub globals: HashMap<String, usize>,
    pub functions: HashMap<String, Vec<usize>>,
}

/// One resolved import edge.
#[derive(Debug)]
pub(crate) struct ResolvedImport {
    pub target: usize,
    pub span: Span,
}

/// A resolved struct or union item.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum TypeItem {
    Struct { module: usize, index: usize },
    Union { module: usize, index: usize },
}

/// A resolved global item.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct GlobalItem {
    pub module: usize,
    pub index: usize,
}

/// A resolved function item.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct FnItem {
    pub module: usize,
    pub index: usize,
}

/// The validated module graph.
#[derive(Debug)]
pub(crate) struct ResolvedProgram<'a> {
    pub modules: Vec<ResolvedModule<'a>>,
    pub module_by_name: HashMap<String, usize>,
    pub entry_module: usize,
    pub entry_fn: usize,
}

impl<'a> ResolvedProgram<'a> {
    pub(crate) fn module_name(&self, index: usize) -> &str {
        &self.modules[index].ast.name
    }

    pub(crate) fn resolve_module_ref(
        &self,
        current: usize,
        module: Option<&str>,
        span: &Span,
    ) -> Result<usize, TypeError> {
        let Some(name) = module else {
            return Ok(current);
        };
        if name == self.module_name(current) {
            return Ok(current);
        }
        if let Some(target) = self.modules[current].import_map.get(name) {
            return Ok(*target);
        }
        if let Some(target) = self.module_by_name.get(name) {
            return Ok(*target);
        }
        Err(TypeError::new(
            span.clone(),
            TypeErrorKind::MissingModule,
            format!("module `{name}` is not imported or does not exist"),
        ))
    }

    pub(crate) fn visible(&self, current: usize, target: usize, vis: Visibility) -> bool {
        current == target || vis == Visibility::Public
    }

    pub(crate) fn lookup_type(
        &self,
        current: usize,
        module: Option<&str>,
        name: &str,
        span: &Span,
    ) -> Result<TypeItem, TypeError> {
        if let Some(module_name) = module {
            let target = self.resolve_module_ref(current, Some(module_name), span)?;
            let target_module = &self.modules[target];
            if let Some(index) = target_module.structs.get(name) {
                let def = &target_module.ast.structs[*index];
                if !self.visible(current, target, def.vis) {
                    return Err(TypeError::new(
                        span.clone(),
                        TypeErrorKind::PrivateAccess,
                        format!("struct `{module_name}::{name}` is private"),
                    ));
                }
                return Ok(TypeItem::Struct {
                    module: target,
                    index: *index,
                });
            }
            if let Some(index) = target_module.unions.get(name) {
                let def = &target_module.ast.unions[*index];
                if !self.visible(current, target, def.vis) {
                    return Err(TypeError::new(
                        span.clone(),
                        TypeErrorKind::PrivateAccess,
                        format!("union `{module_name}::{name}` is private"),
                    ));
                }
                return Ok(TypeItem::Union {
                    module: target,
                    index: *index,
                });
            }
            return Err(TypeError::new(
                span.clone(),
                TypeErrorKind::UndefinedType,
                format!("type `{module_name}::{name}` not found"),
            ));
        }

        let local = &self.modules[current];
        if let Some(index) = local.structs.get(name) {
            return Ok(TypeItem::Struct {
                module: current,
                index: *index,
            });
        }
        if let Some(index) = local.unions.get(name) {
            return Ok(TypeItem::Union {
                module: current,
                index: *index,
            });
        }

        let mut found = Vec::new();
        for target in &local.imported_modules {
            let module_ref = &self.modules[*target];
            if let Some(index) = module_ref.structs.get(name) {
                if module_ref.ast.structs[*index].vis == Visibility::Public {
                    found.push(TypeItem::Struct {
                        module: *target,
                        index: *index,
                    });
                }
            }
            if let Some(index) = module_ref.unions.get(name) {
                if module_ref.ast.unions[*index].vis == Visibility::Public {
                    found.push(TypeItem::Union {
                        module: *target,
                        index: *index,
                    });
                }
            }
        }
        match found.as_slice() {
            [] => Err(TypeError::new(
                span.clone(),
                TypeErrorKind::UndefinedType,
                format!("type `{name}` not found"),
            )),
            [item] => Ok(*item),
            _ => Err(TypeError::new(
                span.clone(),
                TypeErrorKind::AmbiguousItem,
                format!("type `{name}` is ambiguous across imports"),
            )),
        }
    }

    pub(crate) fn lookup_global(
        &self,
        current: usize,
        module: Option<&str>,
        name: &str,
        span: &Span,
    ) -> Result<GlobalItem, TypeError> {
        if let Some(module_name) = module {
            let target = self.resolve_module_ref(current, Some(module_name), span)?;
            let target_module = &self.modules[target];
            let Some(index) = target_module.globals.get(name) else {
                return Err(TypeError::new(
                    span.clone(),
                    TypeErrorKind::UndefinedGlobal,
                    format!("global `{module_name}::{name}` not found"),
                ));
            };
            let def = &target_module.ast.globals[*index];
            if !self.visible(current, target, def.vis) {
                return Err(TypeError::new(
                    span.clone(),
                    TypeErrorKind::PrivateAccess,
                    format!("global `{module_name}::{name}` is private"),
                ));
            }
            return Ok(GlobalItem {
                module: target,
                index: *index,
            });
        }

        let local = &self.modules[current];
        if let Some(index) = local.globals.get(name) {
            return Ok(GlobalItem {
                module: current,
                index: *index,
            });
        }
        let mut found = Vec::new();
        for target in &local.imported_modules {
            let module_ref = &self.modules[*target];
            if let Some(index) = module_ref.globals.get(name) {
                if module_ref.ast.globals[*index].vis == Visibility::Public {
                    found.push(GlobalItem {
                        module: *target,
                        index: *index,
                    });
                }
            }
        }
        match found.as_slice() {
            [] => Err(TypeError::new(
                span.clone(),
                TypeErrorKind::UndefinedGlobal,
                format!("global `{name}` not found"),
            )),
            [item] => Ok(*item),
            _ => Err(TypeError::new(
                span.clone(),
                TypeErrorKind::AmbiguousItem,
                format!("global `{name}` is ambiguous across imports"),
            )),
        }
    }

    pub(crate) fn function_candidates(
        &self,
        current: usize,
        module: Option<&str>,
        name: &str,
        span: &Span,
    ) -> Result<Vec<FnItem>, TypeError> {
        if let Some(module_name) = module {
            let target = self.resolve_module_ref(current, Some(module_name), span)?;
            let target_module = &self.modules[target];
            let Some(indices) = target_module.functions.get(name) else {
                let hint =
                    closest_match(name, target_module.functions.keys().map(String::as_str), 2)
                        .map_or_else(String::new, |candidate| {
                            format!(" — did you mean '{}'?", humanize(candidate))
                        });
                return Err(TypeError::new(
                    span.clone(),
                    TypeErrorKind::UndefinedFunction,
                    format!("function `{module_name}::{name}` not found{hint}"),
                ));
            };
            if target != current {
                let public = indices
                    .iter()
                    .filter(|index| target_module.ast.functions[**index].vis == Visibility::Public)
                    .map(|index| FnItem {
                        module: target,
                        index: *index,
                    })
                    .collect::<Vec<_>>();
                if public.is_empty() {
                    return Err(TypeError::new(
                        span.clone(),
                        TypeErrorKind::PrivateAccess,
                        format!("function `{module_name}::{name}` is private"),
                    ));
                }
                return Ok(public);
            }
            return Ok(indices
                .iter()
                .map(|index| FnItem {
                    module: target,
                    index: *index,
                })
                .collect());
        }

        let mut items = Vec::new();
        let mut seen = HashSet::new();
        let local = &self.modules[current];
        if let Some(indices) = local.functions.get(name) {
            for index in indices {
                if seen.insert((current, *index)) {
                    items.push(FnItem {
                        module: current,
                        index: *index,
                    });
                }
            }
        }
        for target in &local.imported_modules {
            let module_ref = &self.modules[*target];
            if let Some(indices) = module_ref.functions.get(name) {
                for index in indices {
                    if module_ref.ast.functions[*index].vis == Visibility::Public
                        && seen.insert((*target, *index))
                    {
                        items.push(FnItem {
                            module: *target,
                            index: *index,
                        });
                    }
                }
            }
        }
        if items.is_empty() {
            let mut candidates = local
                .functions
                .keys()
                .map(String::as_str)
                .collect::<Vec<_>>();
            for target in &local.imported_modules {
                let module_ref = &self.modules[*target];
                candidates.extend(
                    module_ref
                        .functions
                        .iter()
                        .filter(|(_, indices)| {
                            indices.iter().any(|index| {
                                module_ref.ast.functions[*index].vis == Visibility::Public
                            })
                        })
                        .map(|(name, _)| name.as_str()),
                );
            }
            let hint = closest_match(name, candidates, 2).map_or_else(String::new, |candidate| {
                format!(" — did you mean '{}'?", humanize(candidate))
            });
            return Err(TypeError::new(
                span.clone(),
                TypeErrorKind::UndefinedFunction,
                format!("function `{name}` not found{hint}"),
            ));
        }
        Ok(items)
    }

    pub(crate) fn struct_ast(&self, module: usize, index: usize) -> &'a AstStruct {
        &self.modules[module].ast.structs[index]
    }

    pub(crate) fn union_ast(&self, module: usize, index: usize) -> &'a AstUnion {
        &self.modules[module].ast.unions[index]
    }
}

/// Resolves the module graph and validates structural invariants.
pub(crate) fn resolve_program(program: &Program) -> Result<ResolvedProgram<'_>, Vec<TypeError>> {
    let mut errors = Vec::new();
    let mut module_by_name = HashMap::new();
    for (index, module) in program.modules.iter().enumerate() {
        if module_by_name.insert(module.name.clone(), index).is_some() {
            errors.push(TypeError::new(
                Span::new(module.name.clone(), 1, 1),
                TypeErrorKind::DuplicateModule,
                format!("duplicate module `{}`", module.name),
            ));
        }
    }

    let mut modules = Vec::with_capacity(program.modules.len());
    for (index, module) in program.modules.iter().enumerate() {
        modules.push(build_module(index, module, &module_by_name, &mut errors));
    }

    detect_import_cycles(&modules, &mut errors);

    for module in &modules {
        for import in &module.imports {
            if import.target == module.index {
                errors.push(TypeError::new(
                    import.span.clone(),
                    TypeErrorKind::CircularImport,
                    format!("module `{}` imports itself", module.ast.name),
                ));
            }
        }
    }

    let entry_module = module_by_name.get(&program.entry_module).copied();
    if entry_module.is_none() {
        errors.push(TypeError::new(
            Span::new(program.entry_module.clone(), 1, 1),
            TypeErrorKind::MissingEntryModule,
            format!("entry module `{}` not found", program.entry_module),
        ));
    }

    let mut entry_fn = None;
    if let Some(entry_module) = entry_module {
        let module = &modules[entry_module];
        match module.functions.get(&program.entry_fn) {
            None => errors.push(TypeError::new(
                Span::new(program.entry_module.clone(), 1, 1),
                TypeErrorKind::MissingEntryFunction,
                format!("entry function `{}` not found", program.entry_fn),
            )),
            Some(indices) if indices.len() == 1 => {
                let index = indices[0];
                if !module.ast.functions[index].type_params.is_empty() {
                    errors.push(TypeError::new(
                        module.ast.functions[index].span.clone(),
                        TypeErrorKind::InvalidEntrySignature,
                        "entry function must not be generic".to_string(),
                    ));
                }
                entry_fn = Some(index);
            }
            Some(_) => errors.push(TypeError::new(
                Span::new(program.entry_module.clone(), 1, 1),
                TypeErrorKind::DuplicateItem,
                format!("entry function `{}` is overloaded", program.entry_fn),
            )),
        }
    }

    if !errors.is_empty() {
        return Err(errors);
    }

    Ok(ResolvedProgram {
        modules,
        module_by_name,
        entry_module: entry_module.expect("checked entry module"),
        entry_fn: entry_fn.expect("checked entry function"),
    })
}

fn build_module<'a>(
    index: usize,
    ast: &'a AstModule,
    module_by_name: &HashMap<String, usize>,
    errors: &mut Vec<TypeError>,
) -> ResolvedModule<'a> {
    let mut structs = HashMap::new();
    let mut unions = HashMap::new();
    let mut globals = HashMap::new();
    let mut functions: HashMap<String, Vec<usize>> = HashMap::new();
    let mut import_map = HashMap::new();
    let mut imports = Vec::new();
    let mut imported_modules = Vec::new();

    for import in &ast.imports {
        let Some(target) = module_by_name.get(&import.module).copied() else {
            errors.push(TypeError::new(
                import.span.clone(),
                TypeErrorKind::MissingModule,
                format!("imported module `{}` not found", import.module),
            ));
            continue;
        };
        let key = import
            .alias
            .clone()
            .unwrap_or_else(|| import.module.clone());
        if import_map.insert(key.clone(), target).is_some() {
            errors.push(TypeError::new(
                import.span.clone(),
                TypeErrorKind::DuplicateModuleAlias,
                format!("duplicate import binding `{key}`"),
            ));
        }
        if !imported_modules.contains(&target) {
            imported_modules.push(target);
        }
        imports.push(ResolvedImport {
            target,
            span: import.span.clone(),
        });
    }

    let mut type_names = HashSet::new();
    for (struct_index, def) in ast.structs.iter().enumerate() {
        if !type_names.insert(def.name.clone()) {
            errors.push(TypeError::new(
                def.span.clone(),
                TypeErrorKind::DuplicateItem,
                format!("duplicate type `{}`", def.name),
            ));
        }
        if structs.insert(def.name.clone(), struct_index).is_some() {
            errors.push(TypeError::new(
                def.span.clone(),
                TypeErrorKind::DuplicateItem,
                format!("duplicate struct `{}`", def.name),
            ));
        }
        validate_unique_names(
            def.type_params.iter().map(String::as_str),
            &def.span,
            "type parameter",
            errors,
        );
        validate_unique_names(
            def.fields.iter().map(|(name, _)| name.as_str()),
            &def.span,
            "field",
            errors,
        );
    }
    for (union_index, def) in ast.unions.iter().enumerate() {
        if !type_names.insert(def.name.clone()) {
            errors.push(TypeError::new(
                def.span.clone(),
                TypeErrorKind::DuplicateItem,
                format!("duplicate type `{}`", def.name),
            ));
        }
        if unions.insert(def.name.clone(), union_index).is_some() {
            errors.push(TypeError::new(
                def.span.clone(),
                TypeErrorKind::DuplicateItem,
                format!("duplicate union `{}`", def.name),
            ));
        }
        validate_unique_names(
            def.type_params.iter().map(String::as_str),
            &def.span,
            "type parameter",
            errors,
        );
        validate_unique_names(
            def.fields.iter().map(|(name, _)| name.as_str()),
            &def.span,
            "member",
            errors,
        );
    }

    for (global_index, def) in ast.globals.iter().enumerate() {
        if globals.insert(def.name.clone(), global_index).is_some() {
            errors.push(TypeError::new(
                def.span.clone(),
                TypeErrorKind::DuplicateItem,
                format!("duplicate global `{}`", def.name),
            ));
        }
    }

    for (fn_index, def) in ast.functions.iter().enumerate() {
        validate_unique_names(
            def.type_params.iter().map(String::as_str),
            &def.span,
            "type parameter",
            errors,
        );
        validate_unique_names(
            def.params.iter().map(|(name, _)| name.as_str()),
            &def.span,
            "parameter",
            errors,
        );
        functions
            .entry(def.name.clone())
            .or_default()
            .push(fn_index);
    }

    for indices in functions.values() {
        for (offset, left) in indices.iter().enumerate() {
            let left_sig = canonical_signature(&ast.functions[*left]);
            for right in &indices[offset + 1..] {
                if left_sig == canonical_signature(&ast.functions[*right]) {
                    errors.push(TypeError::new(
                        ast.functions[*right].span.clone(),
                        TypeErrorKind::DuplicateItem,
                        format!(
                            "function `{}` has a duplicate parameter signature",
                            ast.functions[*right].name
                        ),
                    ));
                }
            }
        }
    }

    ResolvedModule {
        ast,
        index,
        imports,
        import_map,
        imported_modules,
        structs,
        unions,
        globals,
        functions,
    }
}

fn validate_unique_names<'a>(
    names: impl Iterator<Item = &'a str>,
    span: &Span,
    what: &str,
    errors: &mut Vec<TypeError>,
) {
    let mut seen = HashSet::new();
    for name in names {
        if !seen.insert(name.to_string()) {
            errors.push(TypeError::new(
                span.clone(),
                TypeErrorKind::DuplicateItem,
                format!("duplicate {what} `{name}`"),
            ));
        }
    }
}

fn canonical_signature(function: &crate::ast::AstFn) -> Vec<String> {
    let params = function
        .type_params
        .iter()
        .enumerate()
        .map(|(index, name)| (name.clone(), index))
        .collect::<HashMap<_, _>>();
    function
        .params
        .iter()
        .map(|(_, ty)| canonical_ast_type(ty, &params))
        .collect()
}

fn canonical_ast_type(ty: &AstType, params: &HashMap<String, usize>) -> String {
    match ty {
        AstType::Void => "void".to_string(),
        AstType::Bool => "bool".to_string(),
        AstType::I8 => "i8".to_string(),
        AstType::I16 => "i16".to_string(),
        AstType::I32 => "i32".to_string(),
        AstType::I64 => "i64".to_string(),
        AstType::U8 => "u8".to_string(),
        AstType::U16 => "u16".to_string(),
        AstType::U32 => "u32".to_string(),
        AstType::U64 => "u64".to_string(),
        AstType::F32 => "f32".to_string(),
        AstType::F64 => "f64".to_string(),
        AstType::Ptr(inner) => format!("ptr<{}>", canonical_ast_type(inner, params)),
        AstType::Array(inner, count) => {
            format!("arr<{};{count}>", canonical_ast_type(inner, params))
        }
        AstType::Named {
            module,
            name,
            type_args,
        } => {
            let name = if module.is_none() {
                params
                    .get(name)
                    .map_or_else(|| name.clone(), |index| format!("${index}"))
            } else {
                name.clone()
            };
            let args = type_args
                .iter()
                .map(|arg| canonical_ast_type(arg, params))
                .collect::<Vec<_>>()
                .join(",");
            format!("{module:?}::{name}<{args}>")
        }
        AstType::FnPtr {
            params: fn_params,
            ret,
        } => {
            let param_text = fn_params
                .iter()
                .map(|param| canonical_ast_type(param, params))
                .collect::<Vec<_>>()
                .join(",");
            format!("fnptr<{param_text}->{}>", canonical_ast_type(ret, params))
        }
    }
}

fn detect_import_cycles(modules: &[ResolvedModule<'_>], errors: &mut Vec<TypeError>) {
    fn visit(
        index: usize,
        modules: &[ResolvedModule<'_>],
        state: &mut [u8],
        stack: &mut Vec<usize>,
        reported: &mut HashSet<usize>,
        errors: &mut Vec<TypeError>,
    ) {
        state[index] = 1;
        stack.push(index);
        for import in &modules[index].imports {
            let target = import.target;
            match state[target] {
                0 => visit(target, modules, state, stack, reported, errors),
                1 if reported.insert(target) => {
                    let start = stack.iter().position(|item| *item == target).unwrap_or(0);
                    let mut path = stack[start..]
                        .iter()
                        .map(|item| modules[*item].ast.name.clone())
                        .collect::<Vec<_>>();
                    path.push(modules[target].ast.name.clone());
                    errors.push(TypeError::new(
                        import.span.clone(),
                        TypeErrorKind::CircularImport,
                        format!("circular import: {}", path.join(" -> ")),
                    ));
                }
                1 => {}
                _ => {}
            }
        }
        stack.pop();
        state[index] = 2;
    }

    let mut state = vec![0u8; modules.len()];
    let mut reported = HashSet::new();
    for index in 0..modules.len() {
        if state[index] == 0 {
            visit(
                index,
                modules,
                &mut state,
                &mut Vec::new(),
                &mut reported,
                errors,
            );
        }
    }
}
