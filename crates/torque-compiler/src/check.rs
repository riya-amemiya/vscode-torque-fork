// Copyright 2026 Riya Amemiya.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     https://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use std::collections::HashMap;

use crate::ast::*;
use crate::diagnostic::{Definition, Diagnostic, IncludeInfo, SymbolInfo};
use crate::span::Span;
use crate::types::{FieldInfo, TypeId, TypeKind, TypeStore};
use serde::Serialize;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileAnalysis {
    pub uri: String,
    pub diagnostics: Vec<Diagnostic>,
    pub symbols: Vec<SymbolInfo>,
    pub includes: Vec<IncludeInfo>,
    pub definitions: Vec<Definition>,
}

#[derive(Clone, Debug)]
struct Binding {
    name: String,
    kind: String,
    span: Span,
    uri: String,
    ty: TypeId,
    operator_name: Option<String>,
    generic_params: Vec<String>,
    param_types: Vec<TypeId>,
    return_type: TypeId,
    implicit_count: usize,
    container: Option<String>,
    detail: Option<String>,
}

enum CallMatch {
    Match { ret: TypeId, score: i32 },
    Skip,
    InferFail(String),
}

struct Scope {
    values: HashMap<String, Binding>,
}

struct Checker {
    uris: Vec<String>,
    types: TypeStore,
    diagnostics: Vec<Diagnostic>,
    definitions: Vec<Definition>,
    symbols: Vec<(u32, SymbolInfo)>,
    includes: Vec<(u32, IncludeInfo)>,
    types_by_name: HashMap<String, TypeId>,
    callables: HashMap<String, Vec<usize>>,
    bindings: Vec<Binding>,
    scopes: Vec<Scope>,
    error_ty: TypeId,
    void_ty: TypeId,
    never_ty: TypeId,
    int_lit_ty: TypeId,
    string_lit_ty: TypeId,
    current_return: TypeId,
}

const PRELUDE_TYPES: &[&str] = &[
    "bool",
    "Object",
    "Smi",
    "HeapObject",
    "HeapNumber",
    "String",
    "Boolean",
    "True",
    "False",
    "Null",
    "Undefined",
    "Context",
    "NativeContext",
    "JSAny",
    "JSObject",
    "JSReceiver",
    "Map",
    "Name",
    "Oddball",
    "Numeric",
    "Number",
    "float64",
    "int31",
    "int32",
    "uint32",
    "intptr",
    "uintptr",
    "bint",
    "Tagged",
];

impl Checker {
    fn new(uris: Vec<String>) -> Self {
        let mut types = TypeStore::new();
        let error_ty = types.intern(TypeKind::Error, Span::dummy());
        let void_ty = types.intern(TypeKind::Void, Span::dummy());
        let never_ty = types.intern(TypeKind::Never, Span::dummy());
        let int_lit_ty = types.intern(TypeKind::IntegerLiteral, Span::dummy());
        let string_lit_ty = types.intern(TypeKind::StringLiteral, Span::dummy());
        let mut types_by_name = HashMap::new();
        types_by_name.insert("void".into(), void_ty);
        types_by_name.insert("never".into(), never_ty);
        types_by_name.insert("IntegerLiteral".into(), int_lit_ty);
        Self {
            uris,
            types,
            diagnostics: Vec::new(),
            definitions: Vec::new(),
            symbols: Vec::new(),
            includes: Vec::new(),
            types_by_name,
            callables: HashMap::new(),
            bindings: Vec::new(),
            scopes: vec![Scope {
                values: HashMap::new(),
            }],
            error_ty,
            void_ty,
            never_ty,
            int_lit_ty,
            string_lit_ty,
            current_return: void_ty,
        }
    }

    fn uri(&self, file: u32) -> String {
        self.uris
            .get(file as usize)
            .cloned()
            .unwrap_or_else(|| "torque:unknown".into())
    }

    fn error(&mut self, span: Span, message: impl Into<String>) {
        self.diagnostics.push(Diagnostic::error(span, message));
    }

    fn define(&mut self, from: Span, to: Span, to_uri: String) {
        if from.file == 0 && from.start == 0 && from.end == 0 {
            return;
        }
        if to_uri == "torque:prelude" {
            return;
        }
        self.definitions.push(Definition {
            from_start: from.start,
            from_end: from.end,
            to_uri,
            to_start: to.start,
            to_end: to.end,
            from_file: from.file,
        });
    }

    fn push_scope(&mut self) {
        self.scopes.push(Scope {
            values: HashMap::new(),
        });
    }

    fn pop_scope(&mut self) {
        self.scopes.pop();
    }

    fn insert_value(&mut self, binding: Binding) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.values.insert(binding.name.clone(), binding);
        }
    }

    fn lookup_value(&self, name: &str) -> Option<&Binding> {
        for scope in self.scopes.iter().rev() {
            if let Some(binding) = scope.values.get(name) {
                return Some(binding);
            }
        }
        None
    }

    #[allow(clippy::too_many_arguments)]
    fn push_symbol(
        &mut self,
        file: u32,
        name: String,
        kind: &str,
        start: u32,
        end: u32,
        container_name: Option<String>,
        detail: Option<String>,
    ) {
        if self.uri(file) == "torque:prelude" {
            return;
        }
        self.symbols.push((
            file,
            SymbolInfo {
                name,
                kind: kind.into(),
                start,
                end,
                container_name,
                detail,
            },
        ));
    }

    fn add_symbol(&mut self, binding: &Binding) {
        if binding.uri == "torque:prelude" {
            return;
        }
        self.push_symbol(
            binding.span.file,
            binding.name.clone(),
            &binding.kind,
            binding.span.start,
            binding.span.end,
            binding.container.clone(),
            binding.detail.clone(),
        );
    }

    fn intern_named(&mut self, name: &str, kind: TypeKind, span: Span) -> TypeId {
        if let Some(id) = self.types_by_name.get(name) {
            return *id;
        }
        let id = self.types.intern(kind, span);
        self.types_by_name.insert(name.to_string(), id);
        id
    }

    fn inject_prelude(&mut self) {
        for name in PRELUDE_TYPES {
            if self.types_by_name.contains_key(*name) {
                continue;
            }
            let parent = match *name {
                "True" | "False" => self.types_by_name.get("Boolean").copied(),
                "Smi" | "HeapObject" => self.types_by_name.get("Object").copied(),
                "HeapNumber" => self.types_by_name.get("HeapObject").copied(),
                "JSReceiver" => self.types_by_name.get("HeapObject").copied(),
                "JSObject" => self.types_by_name.get("JSReceiver").copied(),
                "String" | "Oddball" | "Map" | "Name" => {
                    self.types_by_name.get("HeapObject").copied()
                }
                "Null" | "Undefined" => self.types_by_name.get("Oddball").copied(),
                "NativeContext" => self.types_by_name.get("Context").copied(),
                _ => None,
            };
            let id = self.types.intern(
                TypeKind::Abstract {
                    name: (*name).to_string(),
                    parent,
                    is_constexpr: false,
                },
                Span::dummy(),
            );
            self.types_by_name.insert((*name).to_string(), id);
        }
        self.inject_arguments();
        for name in ["True", "False", "Null", "Undefined"] {
            if self.lookup_value(name).is_none()
                && let Some(ty) = self.types_by_name.get(name).copied()
            {
                self.insert_const(name, ty);
            }
        }
    }

    fn insert_const(&mut self, name: &str, ty: TypeId) {
        let binding = Binding {
            name: name.into(),
            kind: "const".into(),
            span: Span::dummy(),
            uri: "torque:prelude".into(),
            ty,
            operator_name: None,
            generic_params: Vec::new(),
            param_types: Vec::new(),
            return_type: ty,
            implicit_count: 0,
            container: None,
            detail: None,
        };
        self.insert_value(binding);
    }

    fn inject_arguments(&mut self) {
        if self.types_by_name.contains_key("Arguments") {
            return;
        }
        let intptr = self
            .types_by_name
            .get("intptr")
            .copied()
            .unwrap_or(self.error_ty);
        let jsany = self
            .types_by_name
            .get("JSAny")
            .copied()
            .unwrap_or(self.error_ty);
        let arguments_ty = self.types.intern(
            TypeKind::Struct {
                name: "Arguments".into(),
                parent: None,
                fields: vec![
                    FieldInfo {
                        name: "length".into(),
                        ty: intptr,
                        span: Span::dummy(),
                        indexed: false,
                    },
                    FieldInfo {
                        name: "actual_count".into(),
                        ty: intptr,
                        span: Span::dummy(),
                        indexed: false,
                    },
                ],
            },
            Span::dummy(),
        );
        self.types_by_name.insert("Arguments".into(), arguments_ty);
        let index = self.bindings.len();
        self.callables.entry("[]".into()).or_default().push(index);
        self.bindings.push(Binding {
            name: "[]".into(),
            kind: "macro".into(),
            span: Span::dummy(),
            uri: "torque:prelude".into(),
            ty: jsany,
            operator_name: Some("[]".into()),
            generic_params: Vec::new(),
            param_types: vec![arguments_ty, intptr],
            return_type: jsany,
            implicit_count: 0,
            container: None,
            detail: None,
        });
    }

    fn resolve_type_expr(&mut self, expr: &TypeExpr) -> TypeId {
        match expr {
            TypeExpr::Basic {
                is_constexpr,
                namespace,
                name,
                generic_args,
            } => {
                if !namespace.is_empty() {
                    let qualified = format!("{}::{}", namespace.join("::"), name.name);
                    if let Some(id) = self.types_by_name.get(&qualified).copied() {
                        self.define(name.span, self.types.get(id).span, self.uri_for_type(id));
                        let applied = self.apply_type_args(id, generic_args, name.span);
                        if *is_constexpr {
                            return self.intern_constexpr(applied, name.span);
                        }
                        return applied;
                    }
                }
                if let Some(id) = self.types_by_name.get(&name.name).copied() {
                    self.define(name.span, self.types.get(id).span, self.uri_for_type(id));
                    let applied = self.apply_type_args(id, generic_args, name.span);
                    if *is_constexpr {
                        return self.intern_constexpr(applied, name.span);
                    }
                    return applied;
                }
                let id = self.intern_unknown_type(name);
                let applied = self.apply_type_args(id, generic_args, name.span);
                if *is_constexpr {
                    return self.intern_constexpr(applied, name.span);
                }
                applied
            }
            TypeExpr::Union(left, right) => {
                let left_id = self.resolve_type_expr(left);
                let right_id = self.resolve_type_expr(right);
                self.types.union_of(left_id, right_id, expr.span())
            }
            TypeExpr::Function {
                params,
                result,
                span,
            } => {
                let params: Vec<TypeId> =
                    params.iter().map(|ty| self.resolve_type_expr(ty)).collect();
                let result = self.resolve_type_expr(result);
                self.types
                    .intern(TypeKind::Function { params, result }, *span)
            }
            TypeExpr::Reference {
                inner,
                mutable,
                span,
            } => {
                let inner_id = self.resolve_type_expr(inner);
                let name = if *mutable {
                    "MutableReference"
                } else {
                    "ConstReference"
                };
                self.apply_named(name, vec![inner_id], *span)
            }
        }
    }

    fn intern_unknown_type(&mut self, name: &Ident) -> TypeId {
        if name.name.is_empty() {
            return self.error_ty;
        }
        if let Some(id) = self.types_by_name.get(&name.name).copied() {
            return id;
        }
        let id = self.types.intern(
            TypeKind::Abstract {
                name: name.name.clone(),
                parent: None,
                is_constexpr: false,
            },
            name.span,
        );
        self.types_by_name.insert(name.name.clone(), id);
        id
    }

    fn apply_named(&mut self, name: &str, args: Vec<TypeId>, span: Span) -> TypeId {
        let applied_name = self
            .types_by_name
            .get(name)
            .and_then(|id| self.constructor_name(*id).map(str::to_string))
            .unwrap_or_else(|| name.to_string());
        self.types.intern_applied(applied_name, args, span)
    }

    fn apply_type_args(&mut self, base: TypeId, generic_args: &[TypeExpr], span: Span) -> TypeId {
        if generic_args.is_empty() {
            return base;
        }
        let args: Vec<TypeId> = generic_args
            .iter()
            .map(|ty| self.resolve_type_expr(ty))
            .collect();
        let name = match &self.types.get(base).kind {
            TypeKind::Abstract { name, .. }
            | TypeKind::Alias { name, .. }
            | TypeKind::Class { name, .. }
            | TypeKind::Struct { name, .. }
            | TypeKind::Enum { name, .. }
            | TypeKind::GenericParam { name }
            | TypeKind::Applied { name, .. } => name.clone(),
            _ => self.types.name_of(base),
        };
        self.types.intern_applied(name, args, span)
    }

    fn intern_constexpr(&mut self, base: TypeId, span: Span) -> TypeId {
        let base = self.types.unwrap_alias(base);
        if self.types.is_constexpr(base) {
            return base;
        }
        let name = self.types.base_name(base);
        let key = format!("constexpr {name}");
        if let Some(id) = self.types_by_name.get(&key).copied() {
            return id;
        }
        let id = self.types.intern(
            TypeKind::Abstract {
                name: name.clone(),
                parent: Some(base),
                is_constexpr: true,
            },
            span,
        );
        self.types_by_name.insert(key, id);
        id
    }

    fn uri_for_type(&self, id: TypeId) -> String {
        let span = self.types.get(id).span;
        if span.file == 0 && span.start == 0 && span.end == 0 {
            return "torque:prelude".into();
        }
        self.uri(span.file)
    }

    fn predeclare(&mut self, decls: &[Decl], ns: &str) {
        for decl in decls {
            match decl {
                Decl::Namespace { name, body } => {
                    let next = qualify(ns, &name.name);
                    self.add_symbol(&Binding {
                        name: name.name.clone(),
                        kind: "namespace".into(),
                        span: name.span,
                        uri: self.uri(name.span.file),
                        ty: self.void_ty,
                        operator_name: None,
                        generic_params: Vec::new(),
                        param_types: Vec::new(),
                        return_type: self.void_ty,
                        implicit_count: 0,
                        container: nonempty(ns),
                        detail: None,
                    });
                    self.predeclare(body, &next);
                }
                Decl::TypeAlias { name, .. } | Decl::AbstractType { name, .. } => {
                    let q = qualify(ns, &name.name);
                    let id = self.intern_named(
                        &q,
                        TypeKind::Abstract {
                            name: name.name.clone(),
                            parent: None,
                            is_constexpr: false,
                        },
                        name.span,
                    );
                    self.types_by_name.insert(name.name.clone(), id);
                    self.add_symbol(&Binding {
                        name: name.name.clone(),
                        kind: "type".into(),
                        span: name.span,
                        uri: self.uri(name.span.file),
                        ty: id,
                        operator_name: None,
                        generic_params: Vec::new(),
                        param_types: Vec::new(),
                        return_type: id,
                        implicit_count: 0,
                        container: nonempty(ns),
                        detail: None,
                    });
                }
                Decl::Class { name, is_shape, .. } => {
                    let q = qualify(ns, &name.name);
                    let kind = if *is_shape { "shape" } else { "class" };
                    let id = self.intern_named(
                        &q,
                        TypeKind::Class {
                            name: name.name.clone(),
                            parent: None,
                            fields: Vec::new(),
                        },
                        name.span,
                    );
                    self.types_by_name.insert(name.name.clone(), id);
                    self.add_symbol(&Binding {
                        name: name.name.clone(),
                        kind: kind.into(),
                        span: name.span,
                        uri: self.uri(name.span.file),
                        ty: id,
                        operator_name: None,
                        generic_params: Vec::new(),
                        param_types: Vec::new(),
                        return_type: id,
                        implicit_count: 0,
                        container: nonempty(ns),
                        detail: None,
                    });
                }
                Decl::Struct { name, .. } | Decl::BitFieldStruct { name, .. } => {
                    let q = qualify(ns, &name.name);
                    let id = self.intern_named(
                        &q,
                        TypeKind::Struct {
                            name: name.name.clone(),
                            parent: None,
                            fields: Vec::new(),
                        },
                        name.span,
                    );
                    self.types_by_name.insert(name.name.clone(), id);
                    self.add_symbol(&Binding {
                        name: name.name.clone(),
                        kind: "struct".into(),
                        span: name.span,
                        uri: self.uri(name.span.file),
                        ty: id,
                        operator_name: None,
                        generic_params: Vec::new(),
                        param_types: Vec::new(),
                        return_type: id,
                        implicit_count: 0,
                        container: nonempty(ns),
                        detail: None,
                    });
                }
                Decl::Enum { name, .. } => {
                    let q = qualify(ns, &name.name);
                    let id = self.intern_named(
                        &q,
                        TypeKind::Enum {
                            name: name.name.clone(),
                            parent: None,
                            entries: Vec::new(),
                        },
                        name.span,
                    );
                    self.types_by_name.insert(name.name.clone(), id);
                    self.add_symbol(&Binding {
                        name: name.name.clone(),
                        kind: "enum".into(),
                        span: name.span,
                        uri: self.uri(name.span.file),
                        ty: id,
                        operator_name: None,
                        generic_params: Vec::new(),
                        param_types: Vec::new(),
                        return_type: id,
                        implicit_count: 0,
                        container: nonempty(ns),
                        detail: None,
                    });
                }
                Decl::Include { path, span } => {
                    self.includes.push((
                        span.file,
                        IncludeInfo {
                            path: path.clone(),
                            start: span.start,
                            end: span.end,
                        },
                    ));
                }
                _ => {}
            }
        }
    }

    fn bind_decls(&mut self, decls: &[Decl], ns: &str) {
        for decl in decls {
            match decl {
                Decl::Namespace { name, body } => {
                    self.bind_decls(body, &qualify(ns, &name.name));
                }
                Decl::TypeAlias {
                    name,
                    generic_params,
                    ty,
                    ..
                } => {
                    let saved = self.push_generic_params(generic_params);
                    let target = self.resolve_type_expr(ty);
                    self.pop_generic_params(saved);
                    if let Some(id) = self.types_by_name.get(&name.name).copied() {
                        self.types.get_mut(id).kind = TypeKind::Alias {
                            name: name.name.clone(),
                            target,
                        };
                    }
                }
                Decl::AbstractType {
                    name,
                    generic_params,
                    extends,
                    ..
                } => {
                    let saved = self.push_generic_params(generic_params);
                    let parent = extends.as_ref().map(|ty| self.resolve_type_expr(ty));
                    self.pop_generic_params(saved);
                    if let Some(id) = self.types_by_name.get(&name.name).copied()
                        && !matches!(
                            self.types.get(id).kind,
                            TypeKind::Void
                                | TypeKind::Never
                                | TypeKind::IntegerLiteral
                                | TypeKind::StringLiteral
                                | TypeKind::Error
                        )
                    {
                        self.types.get_mut(id).kind = TypeKind::Abstract {
                            name: name.name.clone(),
                            parent,
                            is_constexpr: false,
                        };
                    }
                }
                Decl::Class {
                    name,
                    extends,
                    fields,
                    methods,
                    ..
                } => {
                    let parent = extends.as_ref().map(|ty| self.resolve_type_expr(ty));
                    let mut field_infos = Vec::new();
                    for field in fields {
                        let ty = self.resolve_type_expr(&field.ty);
                        self.define_type_uses(&field.ty);
                        field_infos.push(FieldInfo {
                            name: field.name.name.clone(),
                            ty,
                            span: field.name.span,
                            indexed: field.index.is_some(),
                        });
                        self.push_symbol(
                            field.name.span.file,
                            field.name.name.clone(),
                            "field",
                            field.name.span.start,
                            field.name.span.end,
                            Some(name.name.clone()),
                            None,
                        );
                    }
                    if let Some(id) = self.types_by_name.get(&name.name).copied() {
                        self.types.get_mut(id).kind = TypeKind::Class {
                            name: name.name.clone(),
                            parent,
                            fields: field_infos,
                        };
                    }
                    for method in methods {
                        self.bind_callable(method, Some(&name.name));
                    }
                }
                Decl::Struct {
                    name,
                    generic_params,
                    fields,
                    methods,
                    ..
                } => {
                    let saved_generics = self.push_generic_params(generic_params);
                    let mut field_infos = Vec::new();
                    for field in fields {
                        let ty = self.resolve_type_expr(&field.ty);
                        field_infos.push(FieldInfo {
                            name: field.name.name.clone(),
                            ty,
                            span: field.name.span,
                            indexed: field.index.is_some(),
                        });
                        self.push_symbol(
                            field.name.span.file,
                            field.name.name.clone(),
                            "field",
                            field.name.span.start,
                            field.name.span.end,
                            Some(name.name.clone()),
                            None,
                        );
                    }
                    if let Some(id) = self.types_by_name.get(&name.name).copied() {
                        self.types.get_mut(id).kind = TypeKind::Struct {
                            name: name.name.clone(),
                            parent: None,
                            fields: field_infos,
                        };
                    }
                    for method in methods {
                        self.bind_callable(method, Some(&name.name));
                    }
                    self.pop_generic_params(saved_generics);
                }
                Decl::BitFieldStruct {
                    name,
                    extends,
                    fields,
                    ..
                } => {
                    let parent = Some(self.resolve_type_expr(extends));
                    let mut field_infos = Vec::new();
                    for field in fields {
                        let ty = self.resolve_type_expr(&field.ty);
                        field_infos.push(FieldInfo {
                            name: field.name.name.clone(),
                            ty,
                            span: field.name.span,
                            indexed: field.index.is_some(),
                        });
                        self.push_symbol(
                            field.name.span.file,
                            field.name.name.clone(),
                            "field",
                            field.name.span.start,
                            field.name.span.end,
                            Some(name.name.clone()),
                            None,
                        );
                    }
                    if let Some(id) = self.types_by_name.get(&name.name).copied() {
                        self.types.get_mut(id).kind = TypeKind::Struct {
                            name: name.name.clone(),
                            parent,
                            fields: field_infos,
                        };
                    }
                }
                Decl::Enum {
                    name,
                    extends,
                    entries,
                    ..
                } => {
                    let parent = extends.as_ref().map(|ty| self.resolve_type_expr(ty));
                    let mapped: Vec<(String, Span)> = entries
                        .iter()
                        .map(|(ident, _)| (ident.name.clone(), ident.span))
                        .collect();
                    for (ident, entry_ty) in entries {
                        let ty = if let Some(entry_ty) = entry_ty {
                            self.resolve_type_expr(entry_ty)
                        } else {
                            self.types_by_name
                                .get(&name.name)
                                .copied()
                                .unwrap_or(self.error_ty)
                        };
                        self.insert_value(Binding {
                            name: ident.name.clone(),
                            kind: "const".into(),
                            span: ident.span,
                            uri: self.uri(ident.span.file),
                            ty,
                            operator_name: None,
                            generic_params: Vec::new(),
                            param_types: Vec::new(),
                            return_type: ty,
                            implicit_count: 0,
                            container: Some(name.name.clone()),
                            detail: None,
                        });
                        self.insert_value(Binding {
                            name: format!("{}::{}", name.name, ident.name),
                            kind: "const".into(),
                            span: ident.span,
                            uri: self.uri(ident.span.file),
                            ty,
                            operator_name: None,
                            generic_params: Vec::new(),
                            param_types: Vec::new(),
                            return_type: ty,
                            implicit_count: 0,
                            container: Some(name.name.clone()),
                            detail: None,
                        });
                        self.push_symbol(
                            ident.span.file,
                            ident.name.clone(),
                            "const",
                            ident.span.start,
                            ident.span.end,
                            Some(name.name.clone()),
                            None,
                        );
                    }
                    if let Some(id) = self.types_by_name.get(&name.name).copied() {
                        self.types.get_mut(id).kind = TypeKind::Enum {
                            name: name.name.clone(),
                            parent,
                            entries: mapped,
                        };
                    }
                }
                Decl::Const { name, ty, .. } => {
                    let ty_id = self.resolve_type_expr(ty);
                    let binding = Binding {
                        name: name.name.clone(),
                        kind: "const".into(),
                        span: name.span,
                        uri: self.uri(name.span.file),
                        ty: ty_id,
                        operator_name: None,
                        generic_params: Vec::new(),
                        param_types: Vec::new(),
                        return_type: ty_id,
                        implicit_count: 0,
                        container: nonempty(ns),
                        detail: None,
                    };
                    self.add_symbol(&binding);
                    self.insert_value(binding);
                }
                Decl::Callable(callable) => self.bind_callable(callable, nonempty_ref(ns)),
                Decl::Include { .. } => {}
            }
        }
    }

    fn bind_callable(&mut self, callable: &CallableDecl, container: Option<&str>) {
        let saved_generics = self.push_generic_params(&callable.generic_params);
        let mut param_types = Vec::new();
        for param in callable
            .params
            .implicit
            .iter()
            .chain(callable.params.named.iter())
        {
            param_types.push(self.resolve_type_expr(&param.ty));
        }
        for ty in &callable.params.types_only {
            param_types.push(self.resolve_type_expr(ty));
        }
        let return_type = callable
            .return_type
            .as_ref()
            .map(|ty| self.resolve_type_expr(ty))
            .unwrap_or(self.void_ty);
        let kind = match callable.kind {
            CallableKind::Macro => "macro",
            CallableKind::Builtin => "builtin",
            CallableKind::Runtime => "runtime",
            CallableKind::Intrinsic => "intrinsic",
        };
        let detail = callable.operator_name.clone();
        let binding = Binding {
            name: callable.name.name.clone(),
            kind: kind.into(),
            span: callable.name.span,
            uri: self.uri(callable.name.span.file),
            ty: return_type,
            operator_name: callable.operator_name.clone(),
            generic_params: callable
                .generic_params
                .iter()
                .map(|param| param.name.name.clone())
                .collect(),
            param_types,
            return_type,
            implicit_count: callable.params.implicit.len(),
            container: container.map(str::to_string),
            detail,
        };
        self.add_symbol(&binding);
        let index = self.bindings.len();
        self.callables
            .entry(callable.name.name.clone())
            .or_default()
            .push(index);
        if let Some(op) = &callable.operator_name {
            self.callables.entry(op.clone()).or_default().push(index);
        }
        self.bindings.push(binding);
        self.pop_generic_params(saved_generics);
    }

    fn define_type_uses(&mut self, ty: &TypeExpr) {
        let _ = self.resolve_type_expr(ty);
    }

    fn check_decls(&mut self, decls: &[Decl]) {
        for decl in decls {
            match decl {
                Decl::Namespace { body, .. } => self.check_decls(body),
                Decl::Const { name, ty, init, .. } => {
                    let expected = self.resolve_type_expr(ty);
                    if let Some(init) = init {
                        let found = self.check_expr(init, Some(expected));
                        if !self.can_convert(found, expected) {
                            self.error(
                                init.span(),
                                format!(
                                    "Type '{}' is not assignable to '{}'",
                                    self.types.name_of(found),
                                    self.types.name_of(expected)
                                ),
                            );
                        }
                    }
                    let _ = name;
                }
                Decl::Callable(callable) => self.check_callable(callable, None),
                Decl::Class { name, methods, .. } => {
                    let this_ty = self.types_by_name.get(&name.name).copied();
                    for method in methods {
                        self.check_callable(method, this_ty);
                    }
                }
                Decl::Struct {
                    name,
                    generic_params,
                    methods,
                    ..
                } => {
                    let this_ty = self.types_by_name.get(&name.name).copied();
                    let saved_generics = self.push_generic_params(generic_params);
                    for method in methods {
                        self.check_callable(method, this_ty);
                    }
                    self.pop_generic_params(saved_generics);
                }
                _ => {}
            }
        }
    }

    fn check_callable(&mut self, callable: &CallableDecl, this_ty: Option<TypeId>) {
        self.push_scope();
        let saved_generics = self.push_generic_params(&callable.generic_params);
        if let Some(ty) = this_ty {
            self.insert_value(Binding {
                name: "this".into(),
                kind: "const".into(),
                span: callable.name.span,
                uri: self.uri(callable.name.span.file),
                ty,
                operator_name: None,
                generic_params: Vec::new(),
                param_types: Vec::new(),
                return_type: ty,
                implicit_count: 0,
                container: Some(callable.name.name.clone()),
                detail: None,
            });
        }
        for param in callable
            .params
            .implicit
            .iter()
            .chain(callable.params.named.iter())
        {
            let ty = self.resolve_type_expr(&param.ty);
            self.insert_value(Binding {
                name: param.name.name.clone(),
                kind: "const".into(),
                span: param.name.span,
                uri: self.uri(param.name.span.file),
                ty,
                operator_name: None,
                generic_params: Vec::new(),
                param_types: Vec::new(),
                return_type: ty,
                implicit_count: 0,
                container: Some(callable.name.name.clone()),
                detail: None,
            });
            self.push_symbol(
                param.name.span.file,
                param.name.name.clone(),
                "const",
                param.name.span.start,
                param.name.span.end,
                Some(callable.name.name.clone()),
                None,
            );
        }
        for label in &callable.labels {
            self.insert_value(Binding {
                name: label.name.name.clone(),
                kind: "const".into(),
                span: label.name.span,
                uri: self.uri(label.name.span.file),
                ty: self.void_ty,
                operator_name: None,
                generic_params: Vec::new(),
                param_types: Vec::new(),
                return_type: self.never_ty,
                implicit_count: 0,
                container: Some(callable.name.name.clone()),
                detail: Some("label".into()),
            });
        }
        if let Some(rest) = &callable.params.rest {
            let ty = self
                .types_by_name
                .get("Arguments")
                .copied()
                .unwrap_or_else(|| {
                    self.types_by_name
                        .get("JSAny")
                        .copied()
                        .unwrap_or(self.error_ty)
                });
            self.insert_value(Binding {
                name: rest.name.clone(),
                kind: "const".into(),
                span: rest.span,
                uri: self.uri(rest.span.file),
                ty,
                operator_name: None,
                generic_params: Vec::new(),
                param_types: Vec::new(),
                return_type: ty,
                implicit_count: 0,
                container: Some(callable.name.name.clone()),
                detail: Some("arguments".into()),
            });
        }
        self.current_return = callable
            .return_type
            .as_ref()
            .map(|ty| self.resolve_type_expr(ty))
            .unwrap_or(self.void_ty);
        if let Some(body) = &callable.body {
            self.check_stmt(body);
        }
        self.pop_generic_params(saved_generics);
        self.pop_scope();
    }

    fn check_stmt(&mut self, stmt: &Stmt) {
        match stmt {
            Stmt::Block { statements, .. } => {
                self.push_scope();
                for statement in statements {
                    self.check_stmt(statement);
                }
                self.pop_scope();
            }
            Stmt::Expr(expr) => {
                let _ = self.check_expr(expr, None);
            }
            Stmt::If {
                cond,
                then_s,
                else_s,
                ..
            } => {
                let _ = self.check_expr(cond, None);
                self.check_stmt(then_s);
                if let Some(else_s) = else_s {
                    self.check_stmt(else_s);
                }
            }
            Stmt::While { cond, body, .. } => {
                let _ = self.check_expr(cond, None);
                self.check_stmt(body);
            }
            Stmt::For {
                init,
                cond,
                step,
                body,
                ..
            } => {
                self.push_scope();
                if let Some(init) = init {
                    self.check_stmt(init);
                }
                if let Some(cond) = cond {
                    let _ = self.check_expr(cond, None);
                }
                if let Some(step) = step {
                    let _ = self.check_expr(step, None);
                }
                self.check_stmt(body);
                self.pop_scope();
            }
            Stmt::Return { value, span } => {
                if let Some(value) = value {
                    let found = self.check_expr(value, Some(self.current_return));
                    if !self.can_convert(found, self.current_return) {
                        self.error(
                            value.span(),
                            format!(
                                "Type '{}' is not assignable to '{}'",
                                self.types.name_of(found),
                                self.types.name_of(self.current_return)
                            ),
                        );
                    }
                } else if !self.types.is_void(self.current_return) {
                    self.error(*span, "Missing return value");
                }
            }
            Stmt::Var {
                name,
                ty,
                init,
                span,
                ..
            } => {
                let inferred = match (ty, init) {
                    (Some(ty), Some(init)) => {
                        let expected = self.resolve_type_expr(ty);
                        let found = self.check_expr(init, Some(expected));
                        if !self.can_convert(found, expected) {
                            self.error(
                                init.span(),
                                format!(
                                    "Type '{}' is not assignable to '{}'",
                                    self.types.name_of(found),
                                    self.types.name_of(expected)
                                ),
                            );
                        }
                        expected
                    }
                    (Some(ty), None) => self.resolve_type_expr(ty),
                    (None, Some(init)) => {
                        let found = self.check_expr(init, None);
                        if self.types.is_error(found) {
                            self.error_ty
                        } else {
                            found
                        }
                    }
                    (None, None) => {
                        self.error(
                            *span,
                            format!(
                                "Cannot infer type of '{}': variable has no type annotation or initializer",
                                name.name
                            ),
                        );
                        self.error_ty
                    }
                };
                let binding = Binding {
                    name: name.name.clone(),
                    kind: "const".into(),
                    span: name.span,
                    uri: self.uri(name.span.file),
                    ty: inferred,
                    operator_name: None,
                    generic_params: Vec::new(),
                    param_types: Vec::new(),
                    return_type: inferred,
                    implicit_count: 0,
                    container: None,
                    detail: None,
                };
                self.add_symbol(&binding);
                self.insert_value(binding);
            }
            Stmt::Typeswitch { expr, cases, .. } => {
                let _ = self.check_expr(expr, None);
                for case in cases {
                    self.push_scope();
                    let ty = self.resolve_type_expr(&case.ty);
                    if let Some(name) = &case.name {
                        self.insert_value(Binding {
                            name: name.name.clone(),
                            kind: "const".into(),
                            span: name.span,
                            uri: self.uri(name.span.file),
                            ty,
                            operator_name: None,
                            generic_params: Vec::new(),
                            param_types: Vec::new(),
                            return_type: ty,
                            implicit_count: 0,
                            container: None,
                            detail: None,
                        });
                        self.push_symbol(
                            name.span.file,
                            name.name.clone(),
                            "const",
                            name.span.start,
                            name.span.end,
                            None,
                            None,
                        );
                    }
                    self.check_stmt(&case.body);
                    self.pop_scope();
                }
            }
            Stmt::Try { body, handlers, .. } => {
                self.push_scope();
                for handler in handlers {
                    match handler {
                        TryHandler::Label { name, .. } => {
                            self.insert_value(Binding {
                                name: name.name.clone(),
                                kind: "const".into(),
                                span: name.span,
                                uri: self.uri(name.span.file),
                                ty: self.void_ty,
                                operator_name: None,
                                generic_params: Vec::new(),
                                param_types: Vec::new(),
                                return_type: self.never_ty,
                                implicit_count: 0,
                                container: None,
                                detail: Some("label".into()),
                            });
                        }
                        TryHandler::Catch { .. } => {}
                    }
                }
                self.check_stmt(body);
                for handler in handlers {
                    match handler {
                        TryHandler::Label { name, params, body } => {
                            self.push_scope();
                            self.define(name.span, name.span, self.uri(name.span.file));
                            for param in &params.named {
                                let ty = self.resolve_type_expr(&param.ty);
                                self.insert_value(Binding {
                                    name: param.name.name.clone(),
                                    kind: "const".into(),
                                    span: param.name.span,
                                    uri: self.uri(param.name.span.file),
                                    ty,
                                    operator_name: None,
                                    generic_params: Vec::new(),
                                    param_types: Vec::new(),
                                    return_type: ty,
                                    implicit_count: 0,
                                    container: None,
                                    detail: None,
                                });
                            }
                            self.check_stmt(body);
                            self.pop_scope();
                        }
                        TryHandler::Catch { names, body } => {
                            self.push_scope();
                            for name in names {
                                self.insert_value(Binding {
                                    name: name.name.clone(),
                                    kind: "const".into(),
                                    span: name.span,
                                    uri: self.uri(name.span.file),
                                    ty: self.error_ty,
                                    operator_name: None,
                                    generic_params: Vec::new(),
                                    param_types: Vec::new(),
                                    return_type: self.error_ty,
                                    implicit_count: 0,
                                    container: None,
                                    detail: None,
                                });
                            }
                            self.check_stmt(body);
                            self.pop_scope();
                        }
                    }
                }
                self.pop_scope();
            }
            Stmt::Goto { label, args } => {
                if let Some(binding) = self.lookup_value(&label.name).cloned() {
                    self.define(label.span, binding.span, binding.uri.clone());
                } else {
                    self.error(label.span, format!("Cannot resolve label '{}'", label.name));
                }
                for arg in args {
                    let _ = self.check_expr(arg, None);
                }
            }
            Stmt::Assert { expr, .. } | Stmt::TailCall(expr) => {
                let _ = self.check_expr(expr, None);
            }
            Stmt::Break { .. } | Stmt::Continue { .. } | Stmt::Debug { .. } => {}
        }
    }

    fn check_expr(&mut self, expr: &Expr, expected: Option<TypeId>) -> TypeId {
        match expr {
            Expr::Ident {
                namespace,
                name,
                generic_args,
            } => {
                let key = if namespace.is_empty() {
                    name.name.clone()
                } else {
                    format!("{}::{}", namespace.join("::"), name.name)
                };
                if let Some(binding) = self
                    .lookup_value(&key)
                    .cloned()
                    .or_else(|| self.lookup_value(&name.name).cloned())
                {
                    self.define(name.span, binding.span, binding.uri);
                    return binding.ty;
                }
                if let Some(indices) = self.callables.get(&key).cloned()
                    && let Some(index) = indices.first()
                {
                    let binding = self.bindings[*index].clone();
                    self.define(name.span, binding.span, binding.uri.clone());
                    return self.function_type_of(&binding);
                }
                if !namespace.is_empty()
                    && let Some(ty_id) = self.types_by_name.get(namespace.last().unwrap()).copied()
                {
                    self.define(
                        name.span,
                        self.types.get(ty_id).span,
                        self.uri_for_type(ty_id),
                    );
                    let _ = generic_args;
                    return ty_id;
                }
                if let Some(id) = self.types_by_name.get(&name.name).copied() {
                    self.define(name.span, self.types.get(id).span, self.uri_for_type(id));
                    let _ = generic_args;
                    return id;
                }
                if matches!(name.name.as_str(), "true" | "false") {
                    return self
                        .types_by_name
                        .get("bool")
                        .copied()
                        .unwrap_or(self.error_ty);
                }
                self.error(name.span, format!("Cannot resolve '{}'", name.name));
                self.error_ty
            }
            Expr::Int { .. } => {
                if let Some(expected) = expected
                    && self.can_convert(self.int_lit_ty, expected)
                {
                    return expected;
                }
                self.int_lit_ty
            }
            Expr::Float { .. } => self
                .types_by_name
                .get("float64")
                .copied()
                .unwrap_or(self.int_lit_ty),
            Expr::String { .. } => self.string_lit_ty,
            Expr::Call {
                callee,
                args,
                otherwise,
                span,
            } => self.check_call(callee, args, otherwise, *span, expected),
            Expr::MethodCall {
                target,
                method,
                args,
                otherwise,
                span,
            } => {
                let recv = self.check_expr(target, None);
                let arg_types: Vec<TypeId> =
                    args.iter().map(|arg| self.check_expr(arg, None)).collect();
                self.check_otherwise(otherwise);
                if let Some(ret) = self.try_resolve_call(
                    &method.name,
                    method.span,
                    &[],
                    &arg_types,
                    *span,
                    expected,
                ) {
                    return ret;
                }
                let mut ufcs = vec![recv];
                ufcs.extend_from_slice(&arg_types);
                self.resolve_call(&method.name, method.span, &[], &ufcs, *span, expected)
            }
            Expr::IntrinsicCall {
                name,
                generic_args,
                args,
            } => {
                let type_args: Vec<TypeId> = generic_args
                    .iter()
                    .map(|ty| self.resolve_type_expr(ty))
                    .collect();
                let arg_types: Vec<TypeId> =
                    args.iter().map(|arg| self.check_expr(arg, None)).collect();
                if let Some(ret) = self.try_resolve_call(
                    &name.name, name.span, &type_args, &arg_types, name.span, None,
                ) {
                    return ret;
                }
                if let Some(ty) = type_args.first() {
                    return *ty;
                }
                self.error(
                    name.span,
                    format!("Cannot resolve intrinsic '{}'", name.name),
                );
                self.error_ty
            }
            Expr::Field { object, field, .. } => self.check_field(object, field),
            Expr::Index {
                object,
                index,
                span,
            } => self.check_index(object, index, *span),
            Expr::Assign {
                target,
                value,
                span,
                ..
            } => self.check_assign(target, value, *span),
            Expr::Conditional {
                cond,
                then_e,
                else_e,
                span,
            } => {
                let _ = self.check_expr(cond, None);
                let left = self.check_expr(then_e, expected);
                let right = self.check_expr(else_e, expected);
                self.types.union_of(left, right, *span)
            }
            Expr::Logical { left, right, .. } => {
                let _ = self.check_expr(left, None);
                let _ = self.check_expr(right, None);
                self.types_by_name
                    .get("bool")
                    .copied()
                    .unwrap_or(self.error_ty)
            }
            Expr::New { ty, fields, .. } | Expr::StructLit { ty, fields, .. } => {
                let id = self.resolve_type_expr(ty);
                for (_, value) in fields {
                    let _ = self.check_expr(value, None);
                }
                id
            }
            Expr::Deref { inner, .. } => {
                let ty = self.check_expr(inner, None);
                self.deref_type(ty)
            }
            Expr::Spread { inner, .. } => self.check_expr(inner, expected),
            Expr::IncDec { target, .. } => self.check_expr(target, expected),
        }
    }

    fn check_call(
        &mut self,
        callee: &Expr,
        args: &[Expr],
        otherwise: &[Stmt],
        span: Span,
        expected: Option<TypeId>,
    ) -> TypeId {
        let (name, name_span, type_args) = match callee {
            Expr::Ident {
                name,
                generic_args,
                namespace,
            } => {
                let type_args: Vec<TypeId> = generic_args
                    .iter()
                    .map(|ty| self.resolve_type_expr(ty))
                    .collect();
                let display = if namespace.is_empty() {
                    name.name.clone()
                } else {
                    format!("{}::{}", namespace.join("::"), name.name)
                };
                (display, name.span, type_args)
            }
            _ => {
                let _ = self.check_expr(callee, None);
                let arg_types: Vec<TypeId> =
                    args.iter().map(|arg| self.check_expr(arg, None)).collect();
                let _ = (arg_types, otherwise, expected);
                return self.error_ty;
            }
        };
        if name == "&" && args.len() == 1 {
            return self.check_address_of(&args[0], expected, span);
        }
        let mut raw_args = Vec::new();
        for arg in args {
            raw_args.push(self.check_expr(arg, None));
        }
        self.check_otherwise(otherwise);
        let result = self.resolve_call(&name, name_span, &type_args, &raw_args, span, expected);
        if let Some(expected) = expected
            && self.can_convert(result, expected)
        {
            return expected;
        }
        result
    }

    fn check_otherwise(&mut self, otherwise: &[Stmt]) {
        for stmt in otherwise {
            if let Stmt::Expr(Expr::Ident {
                name, namespace, ..
            }) = stmt
                && namespace.is_empty()
            {
                if let Some(binding) = self.lookup_value(&name.name).cloned() {
                    self.define(name.span, binding.span, binding.uri);
                } else {
                    self.error(name.span, format!("Cannot resolve '{}'", name.name));
                }
                continue;
            }
            self.check_stmt(stmt);
        }
    }

    fn check_field(&mut self, object: &Expr, field: &Ident) -> TypeId {
        let recv = self.check_expr(object, None);
        if self.types.is_error(recv) {
            return self.error_ty;
        }
        if let Some(found) = self
            .fields_of(recv)
            .into_iter()
            .find(|item| item.name == field.name)
        {
            self.define(field.span, found.span, self.uri(found.span.file));
            return found.ty;
        }
        if self.is_opaque(recv) {
            return self.error_ty;
        }
        let dotted = format!(".{}", field.name);
        if let Some(ret) =
            self.try_resolve_call(&dotted, field.span, &[], &[recv], field.span, None)
        {
            return ret;
        }
        if let Some(ret) =
            self.try_resolve_call(&field.name, field.span, &[], &[], field.span, None)
        {
            return ret;
        }
        if let Some(ret) =
            self.try_resolve_call(&field.name, field.span, &[], &[recv], field.span, None)
        {
            return ret;
        }
        self.error(
            field.span,
            format!(
                "Type '{}' has no field '{}'",
                self.types.name_of(recv),
                field.name
            ),
        );
        self.error_ty
    }

    fn check_index(&mut self, object: &Expr, index: &Expr, span: Span) -> TypeId {
        let idx = self.check_expr(index, None);
        if let Expr::Field {
            object: inner,
            field,
            ..
        } = object
        {
            let recv = self.check_expr(inner, None);
            if self.types.is_error(recv) {
                return self.error_ty;
            }
            let op = format!(".{}[]", field.name);
            if let Some(ret) = self.try_resolve_call(&op, field.span, &[], &[recv, idx], span, None)
            {
                return ret;
            }
            if let Some(found) = self
                .fields_of(recv)
                .into_iter()
                .find(|item| item.name == field.name)
            {
                self.define(field.span, found.span, self.uri(found.span.file));
                if found.indexed {
                    return found.ty;
                }
                if let Some(ret) =
                    self.try_resolve_call("[]", field.span, &[], &[found.ty, idx], span, None)
                {
                    return ret;
                }
                return found.ty;
            }
            if self.is_opaque(recv) {
                return self.error_ty;
            }
            self.error(
                field.span,
                format!(
                    "Type '{}' has no field '{}'",
                    self.types.name_of(recv),
                    field.name
                ),
            );
            return self.error_ty;
        }
        let recv = self.check_expr(object, None);
        if let Some(ret) = self.try_resolve_call("[]", span, &[], &[recv, idx], span, None) {
            return ret;
        }
        if let Some(field) = self.indexed_field(recv) {
            return field.ty;
        }
        recv
    }

    fn check_assign(&mut self, target: &Expr, value: &Expr, _span: Span) -> TypeId {
        if let Expr::Index {
            object,
            index,
            span: index_span,
        } = target
        {
            if let Expr::Field {
                object: inner,
                field,
                ..
            } = object.as_ref()
            {
                let recv = self.check_expr(inner, None);
                let idx = self.check_expr(index, None);
                let found = self.check_expr(value, None);
                if self.types.is_error(recv) {
                    return found;
                }
                let op = format!(".{}[]=", field.name);
                if self
                    .try_resolve_call(&op, field.span, &[], &[recv, idx, found], *index_span, None)
                    .is_some()
                {
                    return found;
                }
            }
            let recv = self.check_expr(object, None);
            if self.is_generic_param(recv) {
                return self.check_expr(value, None);
            }
            if let Some(field) = self.indexed_field(recv) {
                let found = self.check_expr(value, Some(field.ty));
                if !self.can_convert(found, field.ty) {
                    self.error(
                        value.span(),
                        format!(
                            "Type '{}' is not assignable to '{}'",
                            self.types.name_of(found),
                            self.types.name_of(field.ty)
                        ),
                    );
                }
                return field.ty;
            }
        }
        let target_ty = self.check_expr(target, None);
        let found = self.check_expr(value, Some(target_ty));
        if !self.can_convert(found, target_ty) {
            self.error(
                value.span(),
                format!(
                    "Type '{}' is not assignable to '{}'",
                    self.types.name_of(found),
                    self.types.name_of(target_ty)
                ),
            );
        }
        target_ty
    }

    fn resolve_call(
        &mut self,
        name: &str,
        name_span: Span,
        type_args: &[TypeId],
        arg_types: &[TypeId],
        span: Span,
        expected: Option<TypeId>,
    ) -> TypeId {
        if let Some(ret) =
            self.try_resolve_call(name, name_span, type_args, arg_types, span, expected)
        {
            return ret;
        }
        let short = name.rsplit("::").next().unwrap_or(name);
        if let Some(binding) = self.lookup_value(short).cloned()
            && let Some(ret) = self.function_result(binding.ty)
        {
            return ret;
        }
        if matches!(short, "Cast" | "Convert" | "UnsafeCast" | "FromConstexpr") {
            if let Some(ty) = type_args.first() {
                return *ty;
            }
            self.error(
                name_span,
                format!("failed to infer arguments for all type parameters of '{short}'"),
            );
            return self.error_ty;
        }
        if is_operator(short) {
            return self.builtin_operator(short, arg_types, span);
        }
        if let Some(reason) = self.inference_failure(name, type_args, arg_types) {
            self.error(name_span, format!("{reason} for '{short}'"));
            return self.error_ty;
        }
        if self.lookup_value(short).is_none() && !self.callables.contains_key(short) {
            self.error(name_span, format!("Cannot resolve '{short}'"));
        } else {
            self.error(
                name_span,
                format!("Cannot find matching callable '{short}'"),
            );
        }
        self.error_ty
    }

    fn try_resolve_call(
        &mut self,
        name: &str,
        name_span: Span,
        type_args: &[TypeId],
        arg_types: &[TypeId],
        _span: Span,
        expected: Option<TypeId>,
    ) -> Option<TypeId> {
        let (matched, _) = self.collect_matches(name, type_args, arg_types, expected);
        let (binding, ret, _) = matched.first()?;
        self.define(name_span, binding.span, binding.uri.clone());
        if (binding.name == "Cast"
            || binding.name == "Convert"
            || binding.name == "UnsafeCast"
            || binding.name == "FromConstexpr")
            && let Some(ty) = type_args.first()
        {
            return Some(*ty);
        }
        Some(*ret)
    }

    fn inference_failure(
        &mut self,
        name: &str,
        type_args: &[TypeId],
        arg_types: &[TypeId],
    ) -> Option<String> {
        let (_, failure) = self.collect_matches(name, type_args, arg_types, None);
        failure
    }

    fn collect_matches(
        &mut self,
        name: &str,
        type_args: &[TypeId],
        arg_types: &[TypeId],
        expected: Option<TypeId>,
    ) -> (Vec<(Binding, TypeId, i32)>, Option<String>) {
        let short = name.rsplit("::").next().unwrap_or(name);
        let mut candidates: Vec<Binding> = Vec::new();
        if let Some(indices) = self.callables.get(short).cloned() {
            for index in indices {
                candidates.push(self.bindings[index].clone());
            }
        }
        if name != short
            && let Some(indices) = self.callables.get(name).cloned()
        {
            for index in indices {
                candidates.push(self.bindings[index].clone());
            }
        }
        let mut inference_failure: Option<String> = None;
        let mut matched =
            self.match_all_candidates(&candidates, type_args, arg_types, &mut inference_failure);
        if let Some(context) = self.lookup_value("context").map(|binding| binding.ty) {
            let mut extended = arg_types.to_vec();
            extended.push(context);
            matched.extend(self.match_all_candidates(
                &candidates,
                type_args,
                &extended,
                &mut inference_failure,
            ));
        }
        if let Some(expected) = expected {
            let fitting: Vec<_> = matched
                .iter()
                .filter(|(_, ret, _)| self.can_convert(*ret, expected))
                .cloned()
                .collect();
            if !fitting.is_empty() {
                matched = fitting;
            }
        }
        matched.sort_by_key(|item| -item.2);
        (matched, inference_failure)
    }

    fn match_all_candidates(
        &mut self,
        candidates: &[Binding],
        type_args: &[TypeId],
        arg_types: &[TypeId],
        inference_failure: &mut Option<String>,
    ) -> Vec<(Binding, TypeId, i32)> {
        let mut matched = Vec::new();
        for candidate in candidates {
            match self.match_candidate(candidate, type_args, arg_types) {
                CallMatch::Match { ret, score } => matched.push((candidate.clone(), ret, score)),
                CallMatch::Skip => {}
                CallMatch::InferFail(reason) => *inference_failure = Some(reason),
            }
        }
        matched
    }

    fn builtin_operator(&mut self, op: &str, args: &[TypeId], span: Span) -> TypeId {
        let bool_ty = self
            .types_by_name
            .get("bool")
            .copied()
            .unwrap_or(self.error_ty);
        match op {
            "==" | "!=" | "<" | ">" | "<=" | ">=" => {
                if args.len() == 2
                    && !self.comparable(args[0], args[1])
                    && !self.types.is_error(args[0])
                    && !self.types.is_error(args[1])
                {
                    self.error(
                        span,
                        format!(
                            "Cannot compare '{}' with '{}'",
                            self.types.name_of(args[0]),
                            self.types.name_of(args[1])
                        ),
                    );
                }
                bool_ty
            }
            "&&" | "||" => bool_ty,
            "!" => bool_ty,
            _ => args.first().copied().unwrap_or(self.error_ty),
        }
    }

    fn push_generic_params(&mut self, params: &[GenericParam]) -> Vec<(String, Option<TypeId>)> {
        let mut saved = Vec::new();
        for param in params {
            let previous = self.types_by_name.get(&param.name.name).copied();
            saved.push((param.name.name.clone(), previous));
            if !param.is_variable
                && let Some(existing) = previous
                && !matches!(self.types.get(existing).kind, TypeKind::GenericParam { .. })
            {
                continue;
            }
            let id = self
                .types
                .intern_generic_param(param.name.name.clone(), param.name.span);
            self.types_by_name.insert(param.name.name.clone(), id);
        }
        saved
    }

    fn pop_generic_params(&mut self, saved: Vec<(String, Option<TypeId>)>) {
        for (name, previous) in saved.into_iter().rev() {
            self.types_by_name.remove(&name);
            if let Some(previous) = previous {
                self.types_by_name.insert(name, previous);
            }
        }
    }

    fn match_candidate(
        &mut self,
        candidate: &Binding,
        type_args: &[TypeId],
        arg_types: &[TypeId],
    ) -> CallMatch {
        if type_args.len() > candidate.generic_params.len() {
            return CallMatch::Skip;
        }
        if candidate.generic_params.is_empty() && !type_args.is_empty() {
            return CallMatch::Skip;
        }
        for (index, name) in candidate.generic_params.iter().enumerate() {
            let Some(existing) = self.types_by_name.get(name).copied() else {
                continue;
            };
            if matches!(self.types.get(existing).kind, TypeKind::GenericParam { .. }) {
                continue;
            }
            if let Some(arg) = type_args.get(index).copied()
                && self.types.unwrap_alias(arg) != self.types.unwrap_alias(existing)
                && !self.can_convert(arg, existing)
            {
                return CallMatch::Skip;
            }
        }
        let without_implicit = if candidate.implicit_count <= candidate.param_types.len() {
            &candidate.param_types[candidate.implicit_count..]
        } else {
            &candidate.param_types[..]
        };
        let mut best = self.match_params(candidate, type_args, without_implicit, arg_types);
        if candidate.implicit_count > 0 {
            let with_implicit =
                self.match_params(candidate, type_args, &candidate.param_types, arg_types);
            best = prefer_match(best, with_implicit);
        }
        best
    }

    fn match_params(
        &mut self,
        candidate: &Binding,
        type_args: &[TypeId],
        params: &[TypeId],
        arg_types: &[TypeId],
    ) -> CallMatch {
        if arg_types.len() != params.len() {
            return CallMatch::Skip;
        }
        let mut inferred: Vec<Option<TypeId>> = vec![None; candidate.generic_params.len()];
        for (index, ty) in type_args.iter().enumerate() {
            inferred[index] = Some(*ty);
        }
        let mut score = if candidate.generic_params.is_empty() {
            1
        } else {
            2
        };
        for (arg, param) in arg_types.iter().zip(params.iter()) {
            if self.types.is_error(*arg) {
                continue;
            }
            match self.types_unify(*arg, *param, candidate, &mut inferred) {
                Ok(true) => {
                    if self.types.unwrap_alias(*arg) == self.types.unwrap_alias(*param) {
                        score += 2;
                    } else {
                        score += 1;
                    }
                }
                Ok(false) => return CallMatch::Skip,
                Err(reason) => return CallMatch::InferFail(reason),
            }
        }
        if inferred.iter().any(Option::is_none) {
            for (index, found) in inferred.iter().enumerate() {
                if found.is_some() {
                    continue;
                }
                let Some(name) = candidate.generic_params.get(index) else {
                    continue;
                };
                if params
                    .iter()
                    .any(|param| self.types.mentions_generic(*param, name))
                    || self.types.mentions_generic(candidate.return_type, name)
                {
                    return CallMatch::InferFail(
                        "failed to infer arguments for all type parameters".into(),
                    );
                }
            }
        }
        let ret = self.substitute_generics(candidate.return_type, candidate, &inferred);
        CallMatch::Match { ret, score }
    }

    fn types_unify(
        &self,
        arg: TypeId,
        param: TypeId,
        candidate: &Binding,
        inferred: &mut [Option<TypeId>],
    ) -> Result<bool, String> {
        let arg = self.types.unwrap_alias(arg);
        let param = self.types.unwrap_alias(param);
        if self.types.is_error(arg) || self.types.is_error(param) {
            return Ok(true);
        }
        if let Some(name) = self.types.generic_param_name(param)
            && let Some(index) = candidate
                .generic_params
                .iter()
                .position(|item| item == name)
        {
            self.unify_inferred(&mut inferred[index], arg)?;
            return Ok(true);
        }
        if let TypeKind::Applied {
            name: arg_name,
            args: arg_args,
        } = &self.types.get(arg).kind
            && let TypeKind::Applied {
                name: param_name,
                args: param_args,
            } = &self.types.get(param).kind
        {
            if arg_name != param_name || arg_args.len() != param_args.len() {
                return Ok(false);
            }
            let arg_args = arg_args.clone();
            let param_args = param_args.clone();
            for (left, right) in arg_args.into_iter().zip(param_args) {
                if !self.types_unify(left, right, candidate, inferred)? {
                    return Ok(false);
                }
            }
            return Ok(true);
        }
        Ok(self.can_convert(arg, param))
    }

    fn unify_inferred(&self, slot: &mut Option<TypeId>, incoming: TypeId) -> Result<(), String> {
        if self.types.is_error(incoming) {
            return Ok(());
        }
        match *slot {
            None => {
                *slot = Some(incoming);
                Ok(())
            }
            Some(existing) => {
                if self.types.unwrap_alias(existing) == self.types.unwrap_alias(incoming)
                    || self.can_convert(incoming, existing)
                {
                    Ok(())
                } else if self.can_convert(existing, incoming) {
                    *slot = Some(incoming);
                    Ok(())
                } else {
                    Err("found conflicting types for generic parameter".into())
                }
            }
        }
    }

    fn substitute_generics(
        &mut self,
        ty: TypeId,
        candidate: &Binding,
        inferred: &[Option<TypeId>],
    ) -> TypeId {
        let ty = self.types.unwrap_alias(ty);
        if let Some(name) = self.types.generic_param_name(ty)
            && let Some(index) = candidate
                .generic_params
                .iter()
                .position(|item| item == name)
            && let Some(found) = inferred.get(index).copied().flatten()
        {
            return found;
        }
        if let TypeKind::Applied { name, args } = &self.types.get(ty).kind {
            let name = name.clone();
            let args = args.clone();
            let mapped: Vec<TypeId> = args
                .into_iter()
                .map(|arg| self.substitute_generics(arg, candidate, inferred))
                .collect();
            return self.types.intern_applied(name, mapped, Span::dummy());
        }
        if let TypeKind::Union { members } = &self.types.get(ty).kind {
            let members = members.clone();
            let mut result = self.never_ty;
            for member in members {
                let mapped = self.substitute_generics(member, candidate, inferred);
                result = self.types.union_of(result, mapped, Span::dummy());
            }
            return result;
        }
        ty
    }

    fn fields_of(&self, id: TypeId) -> Vec<FieldInfo> {
        let mut seen = Vec::new();
        self.fields_of_walk(id, &mut seen)
    }

    fn fields_of_walk(&self, id: TypeId, seen: &mut Vec<TypeId>) -> Vec<FieldInfo> {
        let id = self.types.unwrap_alias(id);
        if seen.contains(&id) {
            return Vec::new();
        }
        seen.push(id);
        match &self.types.get(id).kind {
            TypeKind::Applied { name, args } => {
                let name = name.clone();
                let args = args.clone();
                let mut fields = if let Some(base) = self.types_by_name.get(&name).copied() {
                    self.fields_of_walk(base, seen)
                } else {
                    Vec::new()
                };
                if fields.is_empty() {
                    for arg in args {
                        fields.extend(self.fields_of_walk(arg, seen));
                    }
                }
                fields
            }
            TypeKind::Abstract {
                parent: Some(parent),
                ..
            } => self.fields_of_walk(*parent, seen),
            TypeKind::Class { fields, parent, .. } => {
                let parent = *parent;
                let fields = fields.clone();
                let mut all = parent
                    .map(|p| self.fields_of_walk(p, seen))
                    .unwrap_or_default();
                all.extend(fields);
                all
            }
            TypeKind::Struct { fields, parent, .. } => {
                let parent = *parent;
                let fields = fields.clone();
                let mut all = parent
                    .map(|p| self.fields_of_walk(p, seen))
                    .unwrap_or_default();
                all.extend(fields);
                all
            }
            TypeKind::Union { members } => {
                let members = members.clone();
                if members.is_empty() {
                    return Vec::new();
                }
                let mut common = self.fields_of_walk(members[0], seen);
                for member in &members[1..] {
                    let fields = self.fields_of_walk(*member, seen);
                    common.retain(|field| fields.iter().any(|other| other.name == field.name));
                }
                common
            }
            _ => Vec::new(),
        }
    }

    fn indexed_field(&self, id: TypeId) -> Option<FieldInfo> {
        self.fields_of(id).into_iter().find(|field| field.indexed)
    }

    fn constructor_name(&self, id: TypeId) -> Option<&str> {
        match &self.types.get(self.types.unwrap_alias(id)).kind {
            TypeKind::Abstract { name, .. }
            | TypeKind::Alias { name, .. }
            | TypeKind::Class { name, .. }
            | TypeKind::Struct { name, .. }
            | TypeKind::Enum { name, .. }
            | TypeKind::Applied { name, .. } => Some(name.as_str()),
            _ => None,
        }
    }

    fn is_generic_param(&self, id: TypeId) -> bool {
        self.types.generic_param_name(id).is_some()
    }

    fn is_opaque(&self, id: TypeId) -> bool {
        if self.is_generic_param(id) {
            return true;
        }
        let id = self.types.unwrap_alias(id);
        match &self.types.get(id).kind {
            TypeKind::Abstract {
                parent: None, name, ..
            } => !PRELUDE_TYPES.contains(&name.as_str()),
            _ => false,
        }
    }

    fn deref_type(&self, ty: TypeId) -> TypeId {
        let ty = self.types.unwrap_alias(ty);
        if let TypeKind::Applied { name, args } = &self.types.get(ty).kind
            && matches!(
                name.as_str(),
                "MutableReference" | "ConstReference" | "Reference"
            )
            && let Some(inner) = args.first()
        {
            return *inner;
        }
        ty
    }

    fn function_type_of(&mut self, binding: &Binding) -> TypeId {
        let params = if binding.implicit_count <= binding.param_types.len() {
            binding.param_types[binding.implicit_count..].to_vec()
        } else {
            binding.param_types.clone()
        };
        self.types.intern(
            TypeKind::Function {
                params,
                result: binding.return_type,
            },
            binding.span,
        )
    }

    fn function_result(&self, ty: TypeId) -> Option<TypeId> {
        match &self.types.get(self.types.unwrap_alias(ty)).kind {
            TypeKind::Function { result, .. } => Some(*result),
            TypeKind::Abstract { name, .. } if name == "BuiltinPtr" => {
                self.types_by_name.get("JSAny").copied()
            }
            _ => None,
        }
    }

    fn check_address_of(&mut self, arg: &Expr, expected: Option<TypeId>, span: Span) -> TypeId {
        match arg {
            Expr::Field { object, field, .. } => {
                let recv = self.check_expr(object, None);
                if self.types.is_error(recv) {
                    return self.error_ty;
                }
                if let Some(found) = self
                    .fields_of(recv)
                    .into_iter()
                    .find(|item| item.name == field.name)
                {
                    self.define(field.span, found.span, self.uri(found.span.file));
                    if found.indexed {
                        return self.apply_named_with_expected(
                            "MutableSlice",
                            found.ty,
                            span,
                            expected,
                        );
                    }
                    return self.apply_named_with_expected(
                        "MutableReference",
                        found.ty,
                        span,
                        expected,
                    );
                }
                if self.is_generic_param(recv) {
                    return expected.unwrap_or(self.error_ty);
                }
                self.error(
                    field.span,
                    format!(
                        "Type '{}' has no field '{}'",
                        self.types.name_of(recv),
                        field.name
                    ),
                );
                self.error_ty
            }
            Expr::Index {
                object,
                index,
                span: index_span,
            } => {
                let elem = self.check_index(object, index, *index_span);
                self.apply_named_with_expected("MutableReference", elem, span, expected)
            }
            _ => {
                let ty = self.check_expr(arg, None);
                self.apply_named_with_expected("MutableReference", ty, span, expected)
            }
        }
    }

    fn apply_named_with_expected(
        &mut self,
        name: &str,
        arg: TypeId,
        span: Span,
        expected: Option<TypeId>,
    ) -> TypeId {
        if let Some(expected) = expected {
            let expected_name = self.types.name_of(expected);
            let matches_slice = name == "MutableSlice"
                && (expected_name.starts_with("MutableSlice")
                    || expected_name.starts_with("ConstSlice")
                    || expected_name.starts_with("Slice"));
            let matches_ref = name == "MutableReference"
                && (expected_name.starts_with("MutableReference")
                    || expected_name.starts_with("ConstReference")
                    || expected_name.starts_with("Reference")
                    || expected_name.starts_with('&'));
            if matches_slice || matches_ref || expected_name.starts_with(name) {
                return expected;
            }
        }
        self.apply_named(name, vec![arg], span)
    }

    fn is_applied(&self, id: TypeId) -> bool {
        matches!(
            self.types.get(self.types.unwrap_alias(id)).kind,
            TypeKind::Applied { .. }
        )
    }

    fn can_convert(&self, from: TypeId, to: TypeId) -> bool {
        if self.types.is_subtype(from, to) {
            return true;
        }
        if self.types.is_integer_literal(from) && self.types.numeric_like(to) {
            return true;
        }
        if self.types.is_string_literal(from) && self.types.is_string_like(to) {
            return true;
        }
        if self.types.is_integer_literal(from) && self.types.name_of(to).contains("bool") {
            return false;
        }
        if self.is_jsany(to) && self.js_value_like(from) {
            return true;
        }
        if self.constructor_name(from).is_some()
            && self.constructor_name(from) == self.constructor_name(to)
            && self.is_applied(from) != self.is_applied(to)
        {
            return true;
        }
        if let TypeKind::Applied { name, .. } = &self.types.get(self.types.unwrap_alias(from)).kind
            && let Some(base) = self.types_by_name.get(name).copied()
            && self.can_convert(base, to)
        {
            return true;
        }
        if let TypeKind::Abstract {
            parent: Some(parent),
            ..
        } = &self.types.get(self.types.unwrap_alias(from)).kind
        {
            let parent = *parent;
            if parent != from && self.can_convert(parent, to) {
                return true;
            }
        }
        if self.types.generic_param_name(from).is_some()
            || self.types.generic_param_name(to).is_some()
        {
            return true;
        }
        if (self.types.is_string_literal(from) || self.types.is_string_like(from))
            && (self.types.is_string_like(to)
                || self.js_value_like(to)
                || matches!(self.types.name_of(to).as_str(), "String" | "Name"))
        {
            return true;
        }
        if self.types.numeric_like(from)
            && self.types.numeric_like(to)
            && !self.types.is_constexpr(from)
            && !self.types.is_constexpr(to)
        {
            return true;
        }
        if self.types.is_constexpr(to)
            && let TypeKind::Abstract {
                parent: Some(base), ..
            } = &self.types.get(self.types.unwrap_alias(to)).kind
            && self.types.is_subtype(from, *base)
        {
            return true;
        }
        if self.is_jsany(from) {
            let name = self.types.name_of(to);
            if name == "Object" || name == "HeapObject" || name == "JSAny" {
                return true;
            }
        }
        if let TypeKind::Function { .. } = &self.types.get(self.types.unwrap_alias(from)).kind {
            let to_name = self.types.name_of(to);
            if to_name == "BuiltinPtr" || to_name.starts_with("builtin(") {
                return true;
            }
        }
        if let TypeKind::Function { .. } = &self.types.get(self.types.unwrap_alias(to)).kind {
            let from_name = self.types.name_of(from);
            if from_name == "BuiltinPtr" || from_name.starts_with("builtin(") {
                return true;
            }
        }
        if self.types.name_of(from) == "Arguments"
            && (self.js_value_like(to) || self.types.name_of(to) == "Object")
        {
            return true;
        }
        false
    }

    fn comparable(&self, left: TypeId, right: TypeId) -> bool {
        self.can_convert(left, right)
            || self.can_convert(right, left)
            || (self.types.numeric_like(left) && self.types.numeric_like(right))
            || (self.js_value_like(left) && self.js_value_like(right))
    }

    fn is_jsany(&self, id: TypeId) -> bool {
        self.types.name_of(id) == "JSAny"
    }

    fn js_value_like(&self, id: TypeId) -> bool {
        if self.types.is_error(id) {
            return true;
        }
        let name = self.types.name_of(id);
        const NAMES: &[&str] = &[
            "JSAny",
            "Object",
            "HeapObject",
            "JSReceiver",
            "JSObject",
            "JSArray",
            "Smi",
            "String",
            "Boolean",
            "Oddball",
            "Null",
            "Undefined",
            "True",
            "False",
            "Number",
            "Numeric",
            "HeapNumber",
            "Name",
            "Map",
        ];
        NAMES.iter().any(|item| name == *item) || name.contains("JS")
    }
}

fn prefer_match(left: CallMatch, right: CallMatch) -> CallMatch {
    match (left, right) {
        (
            CallMatch::Match {
                ret: left_ret,
                score: left_score,
            },
            CallMatch::Match {
                ret: right_ret,
                score: right_score,
            },
        ) => {
            if right_score > left_score {
                CallMatch::Match {
                    ret: right_ret,
                    score: right_score,
                }
            } else {
                CallMatch::Match {
                    ret: left_ret,
                    score: left_score,
                }
            }
        }
        (CallMatch::Match { ret, score }, _) | (_, CallMatch::Match { ret, score }) => {
            CallMatch::Match { ret, score }
        }
        (CallMatch::InferFail(reason), _) | (_, CallMatch::InferFail(reason)) => {
            CallMatch::InferFail(reason)
        }
        _ => CallMatch::Skip,
    }
}

fn nonempty(ns: &str) -> Option<String> {
    if ns.is_empty() {
        None
    } else {
        Some(ns.to_string())
    }
}

fn nonempty_ref(ns: &str) -> Option<&str> {
    if ns.is_empty() { None } else { Some(ns) }
}

fn qualify(ns: &str, name: &str) -> String {
    if ns.is_empty() {
        name.to_string()
    } else {
        format!("{ns}::{name}")
    }
}

fn is_operator(name: &str) -> bool {
    matches!(
        name,
        "==" | "!="
            | "<"
            | ">"
            | "<="
            | ">="
            | "+"
            | "-"
            | "*"
            | "/"
            | "%"
            | "&"
            | "|"
            | "^"
            | "<<"
            | ">>"
            | ">>>"
            | "!"
            | "~"
            | "&&"
            | "||"
    )
}

pub fn check_files(files: &[ParsedFile], parse_diagnostics: Vec<Diagnostic>) -> Vec<FileAnalysis> {
    let uris: Vec<String> = files.iter().map(|file| file.uri.clone()).collect();
    let mut checker = Checker::new(uris);
    for file in files {
        checker.predeclare(&file.decls, "");
    }
    checker.inject_prelude();
    for file in files {
        checker.bind_decls(&file.decls, "");
    }
    for file in files {
        checker.check_decls(&file.decls);
    }
    checker.diagnostics.extend(parse_diagnostics);

    let mut analyses: Vec<FileAnalysis> = files
        .iter()
        .map(|file| FileAnalysis {
            uri: file.uri.clone(),
            diagnostics: Vec::new(),
            symbols: Vec::new(),
            includes: Vec::new(),
            definitions: Vec::new(),
        })
        .collect();

    for diagnostic in checker.diagnostics {
        if let Some(file) = analyses.get_mut(diagnostic.file as usize) {
            file.diagnostics.push(diagnostic);
        }
    }
    for (file, symbol) in checker.symbols {
        if let Some(analysis) = analyses.get_mut(file as usize) {
            analysis.symbols.push(symbol);
        }
    }
    for (file, include) in checker.includes {
        if let Some(analysis) = analyses.get_mut(file as usize) {
            analysis.includes.push(include);
        }
    }
    for definition in checker.definitions {
        if let Some(analysis) = analyses.get_mut(definition.from_file as usize) {
            analysis.definitions.push(definition);
        }
    }
    for analysis in &mut analyses {
        analysis.diagnostics.sort_by_key(|diagnostic| {
            (diagnostic.start, diagnostic.end, diagnostic.message.clone())
        });
        analysis.diagnostics.dedup_by(|left, right| {
            left.start == right.start && left.end == right.end && left.message == right.message
        });
    }
    analyses
}
