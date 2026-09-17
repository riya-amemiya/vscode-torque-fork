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

use crate::span::Span;

#[derive(Clone, Debug)]
pub struct Ident {
    pub name: String,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct Annotation {
    pub name: String,
    pub argument: Option<String>,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub enum TypeExpr {
    Basic {
        is_constexpr: bool,
        namespace: Vec<String>,
        name: Ident,
        generic_args: Vec<TypeExpr>,
    },
    Union(Box<TypeExpr>, Box<TypeExpr>),
    Function {
        params: Vec<TypeExpr>,
        result: Box<TypeExpr>,
        span: Span,
    },
    Reference {
        mutable: bool,
        inner: Box<TypeExpr>,
        span: Span,
    },
}

impl TypeExpr {
    pub fn span(&self) -> Span {
        match self {
            TypeExpr::Basic { name, .. } => name.span,
            TypeExpr::Union(left, right) => left.span().merge(right.span()),
            TypeExpr::Function { span, .. } | TypeExpr::Reference { span, .. } => *span,
        }
    }
}

#[derive(Clone, Debug)]
pub struct NameAndType {
    pub name: Ident,
    pub ty: TypeExpr,
}

#[derive(Clone, Debug)]
pub struct ParamList {
    pub implicit_kind: Option<String>,
    pub implicit: Vec<NameAndType>,
    pub named: Vec<NameAndType>,
    pub types_only: Vec<TypeExpr>,
    pub rest: Option<Ident>,
}

#[derive(Clone, Debug)]
pub struct LabelParam {
    pub name: Ident,
    pub types: Vec<TypeExpr>,
}

#[derive(Clone, Debug)]
pub enum Expr {
    Ident {
        namespace: Vec<String>,
        name: Ident,
        generic_args: Vec<TypeExpr>,
    },
    Int {
        text: String,
        span: Span,
    },
    Float {
        text: String,
        span: Span,
    },
    String {
        value: String,
        span: Span,
    },
    Call {
        callee: Box<Expr>,
        args: Vec<Expr>,
        otherwise: Vec<Stmt>,
        span: Span,
    },
    MethodCall {
        target: Box<Expr>,
        method: Ident,
        args: Vec<Expr>,
        otherwise: Vec<Stmt>,
        span: Span,
    },
    IntrinsicCall {
        name: Ident,
        generic_args: Vec<TypeExpr>,
        args: Vec<Expr>,
    },
    Field {
        object: Box<Expr>,
        field: Ident,
        via_ref: bool,
    },
    Index {
        object: Box<Expr>,
        index: Box<Expr>,
        span: Span,
    },
    Assign {
        target: Box<Expr>,
        op: Option<String>,
        value: Box<Expr>,
        span: Span,
    },
    Conditional {
        cond: Box<Expr>,
        then_e: Box<Expr>,
        else_e: Box<Expr>,
        span: Span,
    },
    Logical {
        op: String,
        left: Box<Expr>,
        right: Box<Expr>,
        span: Span,
    },
    New {
        ty: TypeExpr,
        fields: Vec<(Option<Ident>, Expr)>,
        span: Span,
    },
    StructLit {
        ty: TypeExpr,
        fields: Vec<(Option<Ident>, Expr)>,
        span: Span,
    },
    Deref {
        inner: Box<Expr>,
        span: Span,
    },
    Spread {
        inner: Box<Expr>,
        span: Span,
    },
    IncDec {
        pre: bool,
        inc: bool,
        target: Box<Expr>,
        span: Span,
    },
}

impl Expr {
    pub fn span(&self) -> Span {
        match self {
            Expr::Ident { name, .. } => name.span,
            Expr::Int { span, .. }
            | Expr::Float { span, .. }
            | Expr::String { span, .. }
            | Expr::Call { span, .. }
            | Expr::MethodCall { span, .. }
            | Expr::Index { span, .. }
            | Expr::Assign { span, .. }
            | Expr::Conditional { span, .. }
            | Expr::Logical { span, .. }
            | Expr::New { span, .. }
            | Expr::StructLit { span, .. }
            | Expr::Deref { span, .. }
            | Expr::Spread { span, .. }
            | Expr::IncDec { span, .. } => *span,
            Expr::IntrinsicCall { name, .. } => name.span,
            Expr::Field { field, object, .. } => object.span().merge(field.span),
        }
    }

    pub fn callee_name(&self) -> Option<&Ident> {
        match self {
            Expr::Ident { name, .. } => Some(name),
            Expr::Call { callee, .. } => callee.callee_name(),
            _ => None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct TypeswitchCase {
    pub name: Option<Ident>,
    pub ty: TypeExpr,
    pub body: Box<Stmt>,
}

#[derive(Clone, Debug)]
pub enum TryHandler {
    Label {
        name: Ident,
        params: ParamList,
        body: Box<Stmt>,
    },
    Catch {
        names: Vec<Ident>,
        body: Box<Stmt>,
    },
}

#[derive(Clone, Debug)]
pub enum Stmt {
    Block {
        deferred: bool,
        statements: Vec<Stmt>,
        span: Span,
    },
    Expr(Expr),
    If {
        is_constexpr: bool,
        cond: Expr,
        then_s: Box<Stmt>,
        else_s: Option<Box<Stmt>>,
        span: Span,
    },
    While {
        cond: Expr,
        body: Box<Stmt>,
        span: Span,
    },
    For {
        init: Option<Box<Stmt>>,
        cond: Option<Expr>,
        step: Option<Expr>,
        body: Box<Stmt>,
        span: Span,
    },
    Return {
        value: Option<Expr>,
        span: Span,
    },
    Break {
        span: Span,
    },
    Continue {
        span: Span,
    },
    Goto {
        label: Ident,
        args: Vec<Expr>,
    },
    Debug {
        kind: String,
        span: Span,
    },
    Assert {
        kind: String,
        expr: Expr,
        span: Span,
    },
    TailCall(Expr),
    Var {
        is_const: bool,
        name: Ident,
        ty: Option<TypeExpr>,
        init: Option<Expr>,
        span: Span,
    },
    Typeswitch {
        expr: Expr,
        cases: Vec<TypeswitchCase>,
        span: Span,
    },
    Try {
        body: Box<Stmt>,
        handlers: Vec<TryHandler>,
        span: Span,
    },
}

impl Stmt {
    pub fn span(&self) -> Span {
        match self {
            Stmt::Block { span, .. }
            | Stmt::If { span, .. }
            | Stmt::While { span, .. }
            | Stmt::For { span, .. }
            | Stmt::Return { span, .. }
            | Stmt::Break { span }
            | Stmt::Continue { span }
            | Stmt::Debug { span, .. }
            | Stmt::Assert { span, .. }
            | Stmt::Var { span, .. }
            | Stmt::Typeswitch { span, .. }
            | Stmt::Try { span, .. } => *span,
            Stmt::Expr(expr) | Stmt::TailCall(expr) => expr.span(),
            Stmt::Goto { label, .. } => label.span,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CallableKind {
    Macro,
    Builtin,
    Runtime,
    Intrinsic,
}

#[derive(Clone, Debug)]
pub struct CallableDecl {
    pub annotations: Vec<Annotation>,
    pub transitioning: bool,
    pub javascript: bool,
    pub is_extern: bool,
    pub operator_name: Option<String>,
    pub kind: CallableKind,
    pub name: Ident,
    pub generic_params: Vec<Ident>,
    pub params: ParamList,
    pub return_type: Option<TypeExpr>,
    pub labels: Vec<LabelParam>,
    pub body: Option<Box<Stmt>>,
}

#[derive(Clone, Debug)]
pub struct FieldDecl {
    pub name: Ident,
    pub ty: TypeExpr,
    pub weak: bool,
    pub is_const: bool,
    pub index: Option<Expr>,
    pub bits: Option<i32>,
}

#[derive(Clone, Debug)]
pub enum Decl {
    Namespace {
        name: Ident,
        body: Vec<Decl>,
    },
    TypeAlias {
        name: Ident,
        generic_params: Vec<Ident>,
        ty: TypeExpr,
        annotations: Vec<Annotation>,
    },
    AbstractType {
        name: Ident,
        generic_params: Vec<Ident>,
        extends: Option<TypeExpr>,
        generates: Option<String>,
        constexpr_generates: Option<String>,
        transient: bool,
        annotations: Vec<Annotation>,
    },
    Class {
        name: Ident,
        is_extern: bool,
        is_shape: bool,
        transient: bool,
        extends: Option<TypeExpr>,
        generates: Option<String>,
        fields: Vec<FieldDecl>,
        methods: Vec<CallableDecl>,
        annotations: Vec<Annotation>,
        span: Span,
    },
    Struct {
        name: Ident,
        generic_params: Vec<Ident>,
        fields: Vec<FieldDecl>,
        methods: Vec<CallableDecl>,
        annotations: Vec<Annotation>,
    },
    BitFieldStruct {
        name: Ident,
        extends: TypeExpr,
        fields: Vec<FieldDecl>,
        annotations: Vec<Annotation>,
    },
    Enum {
        name: Ident,
        extends: Option<TypeExpr>,
        entries: Vec<(Ident, Option<TypeExpr>)>,
        is_open: bool,
        annotations: Vec<Annotation>,
    },
    Const {
        name: Ident,
        ty: TypeExpr,
        init: Option<Expr>,
        generates: Option<String>,
        annotations: Vec<Annotation>,
    },
    Callable(CallableDecl),
    Include {
        path: String,
        span: Span,
    },
}

pub struct ParsedFile {
    pub uri: String,
    pub text: String,
    pub file: u32,
    pub decls: Vec<Decl>,
}
