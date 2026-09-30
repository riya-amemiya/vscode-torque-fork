use std::collections::{BTreeMap, BTreeSet};
use std::sync::OnceLock;

use torque_ast::*;
use torque_span::Span;

use crate::earley_parser::{
    Action, Failure, Grammar, LexedToken, ParseResultIterator, Pattern, Rule, RuleId, SymbolId,
    dotted_rule_bases, keywords_by_first_byte, nullable_rules, run_actions, run_earley, run_lexer,
};

const ANNOTATION_ABSTRACT: &str = "@abstract";
const ANNOTATION_HAS_SAME_INSTANCE_TYPE_AS_PARENT: &str = "@hasSameInstanceTypeAsParent";
const ANNOTATION_DO_NOT_GENERATE_CPP_CLASS: &str = "@doNotGenerateCppClass";
const ANNOTATION_DO_NOT_GENERATE_INSTANCE_TYPE_CHECK: &str = "@doNotGenerateInstanceTypeCheck";
const ANNOTATION_CUSTOM_MAP: &str = "@customMap";
const ANNOTATION_CUSTOM_CPP_CLASS: &str = "@customCppClass";
const ANNOTATION_HIGHEST_INSTANCE_TYPE_WITHIN_PARENT: &str =
    "@highestInstanceTypeWithinParentClassRange";
const ANNOTATION_LOWEST_INSTANCE_TYPE_WITHIN_PARENT: &str =
    "@lowestInstanceTypeWithinParentClassRange";
const ANNOTATION_RESERVE_BITS_IN_INSTANCE_TYPE: &str = "@reserveBitsInInstanceType";
const ANNOTATION_INSTANCE_TYPE_VALUE: &str = "@apiExposedInstanceTypeValue";
const ANNOTATION_IF: &str = "@if";
const ANNOTATION_IFNOT: &str = "@ifnot";
const ANNOTATION_EXPORT: &str = "@export";
const ANNOTATION_DO_NOT_GENERATE_CAST: &str = "@doNotGenerateCast";
const ANNOTATION_USE_PARENT_TYPE_CHECKER: &str = "@useParentTypeChecker";
const ANNOTATION_CPP_OBJECT_LAYOUT_DEFINITION: &str = "@cppObjectLayoutDefinition";
const ANNOTATION_CPP_SCOPE: &str = "@cppScope";
const ANNOTATION_SAME_ENUM_VALUE_AS: &str = "@sameEnumValueAs";
const ANNOTATION_CPP_RELAXED_STORE: &str = "@cppRelaxedStore";
const ANNOTATION_CPP_RELAXED_LOAD: &str = "@cppRelaxedLoad";
const ANNOTATION_CPP_RELEASE_STORE: &str = "@cppReleaseStore";
const ANNOTATION_CPP_ACQUIRE_LOAD: &str = "@cppAcquireLoad";
const ANNOTATION_CUSTOM_WEAK_MARKING: &str = "@customWeakMarking";
const ANNOTATION_CUSTOM_INTERFACE_DESCRIPTOR: &str = "@customInterfaceDescriptor";
const ANNOTATION_INCREMENT_USE_COUNTER: &str = "@incrementUseCounter";
const ANNOTATION_SUPPORTS_TSA: &str = "@supportsTSA";

const BUILD_FLAGS: &[(&str, bool)] = &[
    ("V8_EXTERNAL_CODE_SPACE", true),
    ("TAGGED_SIZE_8_BYTES", false),
    ("V8_ENABLE_UNDEFINED_DOUBLE", true),
    ("V8_ENABLE_EXPERIMENTAL_TSA_BUILTINS", false),
    (
        "V8_ENABLE_EXPERIMENTAL_TSA_BUILTINS_WITHOUT_TQ_TO_TSA",
        false,
    ),
    ("V8_INTL_SUPPORT", true),
    ("V8_ENABLE_SWISS_NAME_DICTIONARY", false),
    ("V8_ENABLE_JAVASCRIPT_PROMISE_HOOKS", false),
    ("V8_ENABLE_CONTINUATION_PRESERVED_EMBEDDER_DATA", true),
    ("TRUE_FOR_TESTING", true),
    ("FALSE_FOR_TESTING", false),
    ("V8_SCRIPTORMODULE_LEGACY_LIFETIME", false),
    ("V8_ENABLE_WEBASSEMBLY", true),
    ("WASM_CODE_POINTER_NEEDS_PADDING", false),
    ("V8_ENABLE_SANDBOX", true),
    ("DEBUG", false),
    ("V8_ENABLE_DRUMBRAKE", false),
    ("V8_ENABLE_SEEDED_ARRAY_INDEX_HASH", false),
    ("V8_IS_TSAN", false),
];

const MACHINE_TYPES: &[&str] = &[
    "void",
    "never",
    "int8",
    "uint8",
    "int16",
    "uint16",
    "int31",
    "uint31",
    "int32",
    "uint32",
    "int64",
    "uint64",
    "intptr",
    "uintptr",
    "float16_raw_bits",
    "float32",
    "float64",
    "float64_or_undefined_or_hole",
    "bool",
    "string",
    "bint",
    "char8",
    "char16",
];

const KEYWORD_LIKE_CONSTANTS: &[&str] = &[
    "True",
    "False",
    "TheHole",
    "TdzHole",
    "PromiseHole",
    "Null",
    "Undefined",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MessageKind {
    Error,
    Lint,
}

#[derive(Clone, Debug)]
pub(crate) struct Message {
    pub(crate) kind: MessageKind,
    pub(crate) message: String,
    pub(crate) span: Span,
}

#[derive(Clone, Debug)]
pub(crate) struct ImportCheck {
    pub(crate) path: String,
    pub(crate) span: Span,
    pub(crate) message_index: usize,
}

pub(crate) struct ActionContext<'a> {
    file: u32,
    text: &'a [u8],
    tokens: &'a [LexedToken],
    current: Span,
    messages: Vec<Message>,
    aborted: bool,
    declarations: Vec<Decl>,
    imports: Vec<ImportCheck>,
}

impl ActionContext<'_> {
    pub(crate) fn file(&self) -> u32 {
        self.file
    }

    pub(crate) fn set_current(&mut self, begin: usize, end: usize) {
        self.current = Span::new(self.file, begin, end);
    }

    fn push(&mut self, kind: MessageKind, message: String, span: Span) {
        if self.aborted {
            return;
        }
        self.messages.push(Message {
            kind,
            message,
            span,
        });
    }

    fn error(&mut self, message: impl Into<String>) {
        self.push(MessageKind::Error, message.into(), self.current);
    }

    fn error_at(&mut self, message: impl Into<String>, span: Span) {
        self.push(MessageKind::Error, message.into(), span);
    }

    fn lint(&mut self, message: impl Into<String>) {
        self.push(MessageKind::Lint, message.into(), self.current);
    }

    fn lint_at(&mut self, message: impl Into<String>, span: Span) {
        self.push(MessageKind::Lint, message.into(), span);
    }

    fn report_error(&mut self, message: impl Into<String>) {
        self.error(message);
        self.aborted = true;
    }

    fn token_span(&self, index: usize) -> Span {
        let token = self.tokens[index];
        Span::new(self.file, token.begin, token.end)
    }

    fn text(&self, begin: usize, end: usize) -> String {
        String::from_utf8_lossy(&self.text[begin..end]).into_owned()
    }
}

#[derive(Clone, Debug)]
pub(crate) struct AnnotationParameter {
    string_value: String,
    int_value: i32,
    is_int: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct AnnotationValue {
    name: Ident,
    param: Option<AnnotationParameter>,
}

#[derive(Clone, Debug)]
pub(crate) struct EnumEntryValue {
    name: Ident,
    ty: Option<(TypeExpr, Span)>,
}

type FieldConditions = Vec<(String, bool)>;

#[derive(Debug)]
pub(crate) enum Value {
    Consumed,
    Str(String, Span),
    Strings(Vec<String>),
    Bool(bool),
    Int32(i32),
    Ident(Ident),
    Expr(Expr),
    Stmt(Stmt, Span),
    Decls(Vec<Decl>),
    Method(CallableDecl),
    Type(TypeExpr, Span),
    ImplicitVoid,
    List(Vec<Value>),
    Opt(Option<Box<Value>>),
    Annotation(AnnotationValue),
    AnnotationParameter(AnnotationParameter),
    NameAndType(NameAndType),
    ImplicitParameters(Ident, Vec<NameAndType>),
    ParameterList(ParamList, Option<Span>),
    LabelAndTypes(LabelParam),
    ClassField(FieldDecl, FieldConditions),
    Field(FieldDecl),
    ClassBody(Vec<CallableDecl>, Vec<(FieldDecl, FieldConditions)>),
    EnumEntry(EnumEntryValue),
    NameAndExpression(Option<Ident>, Expr),
    TypeswitchCase(TypeswitchCase),
    TryHandler(TryHandler, Span),
    IncrementDecrement(bool, Span),
    GenericParameter(GenericParam),
    Disabled(Box<Value>),
}

fn split_enabled(value: Value) -> (Value, bool) {
    match value {
        Value::Disabled(inner) => (*inner, false),
        other => (other, true),
    }
}

fn mismatch(expected: &str, value: &Value) -> ! {
    panic!("unexpected parse result: expected {expected}, found {value:?}")
}

impl Value {
    fn into_str(self) -> (String, Span) {
        match self {
            Value::Str(value, span) => (value, span),
            other => mismatch("string", &other),
        }
    }

    fn into_strings(self) -> Vec<String> {
        match self {
            Value::Strings(values) => values,
            other => mismatch("strings", &other),
        }
    }

    fn into_bool(self) -> bool {
        match self {
            Value::Bool(value) => value,
            other => mismatch("bool", &other),
        }
    }

    fn into_int32(self) -> i32 {
        match self {
            Value::Int32(value) => value,
            other => mismatch("int32", &other),
        }
    }

    fn into_ident(self) -> Ident {
        match self {
            Value::Ident(value) => value,
            other => mismatch("identifier", &other),
        }
    }

    fn into_expr(self) -> Expr {
        match self {
            Value::Expr(value) => value,
            other => mismatch("expression", &other),
        }
    }

    fn into_stmt(self) -> (Stmt, Span) {
        match self {
            Value::Stmt(value, pos) => (value, pos),
            other => mismatch("statement", &other),
        }
    }

    fn into_decls(self) -> Vec<Decl> {
        match self {
            Value::Decls(values) => values,
            other => mismatch("declarations", &other),
        }
    }

    fn into_method(self) -> CallableDecl {
        match self {
            Value::Method(value) => value,
            other => mismatch("method", &other),
        }
    }

    fn into_type(self) -> (TypeExpr, Span) {
        match self {
            Value::Type(value, pos) => (value, pos),
            other => mismatch("type", &other),
        }
    }

    fn into_return_type(self) -> Option<TypeExpr> {
        match self {
            Value::Type(value, _) => Some(value),
            Value::ImplicitVoid => None,
            other => mismatch("return type", &other),
        }
    }

    fn into_list(self) -> Vec<Value> {
        match self {
            Value::List(values) => values,
            other => mismatch("list", &other),
        }
    }

    fn into_opt(self) -> Option<Value> {
        match self {
            Value::Opt(value) => value.map(|value| *value),
            other => mismatch("optional", &other),
        }
    }

    fn into_annotations(self) -> Vec<AnnotationValue> {
        self.into_list()
            .into_iter()
            .map(|value| match value {
                Value::Annotation(annotation) => annotation,
                other => mismatch("annotation", &other),
            })
            .collect()
    }

    fn into_parameter_list(self) -> (ParamList, Option<Span>) {
        match self {
            Value::ParameterList(value, implicit_kind_pos) => (value, implicit_kind_pos),
            other => mismatch("parameter list", &other),
        }
    }

    fn into_name_and_type(self) -> NameAndType {
        match self {
            Value::NameAndType(value) => value,
            other => mismatch("name and type", &other),
        }
    }

    fn into_types(self) -> Vec<TypeExpr> {
        self.into_list()
            .into_iter()
            .map(|value| value.into_type().0)
            .collect()
    }

    fn into_exprs(self) -> Vec<Expr> {
        self.into_list().into_iter().map(Value::into_expr).collect()
    }

    fn into_stmts(self) -> Vec<(Stmt, Span)> {
        self.into_list().into_iter().map(Value::into_stmt).collect()
    }

    fn into_generic_parameters(self) -> Vec<GenericParam> {
        self.into_list()
            .into_iter()
            .map(|value| match value {
                Value::GenericParameter(param) => param,
                other => mismatch("generic parameter", &other),
            })
            .collect()
    }

    fn into_labels(self) -> Vec<LabelParam> {
        self.into_list()
            .into_iter()
            .map(|value| match value {
                Value::LabelAndTypes(label) => label,
                other => mismatch("label", &other),
            })
            .collect()
    }

    fn into_named_expressions(self) -> Vec<(Option<Ident>, Expr)> {
        self.into_list()
            .into_iter()
            .map(|value| match value {
                Value::NameAndExpression(name, expression) => (name, expression),
                other => mismatch("named expression", &other),
            })
            .collect()
    }

    fn into_optional_stmt(self) -> Option<Stmt> {
        self.into_opt().map(|value| value.into_stmt().0)
    }

    fn into_optional_string(self) -> Option<String> {
        self.into_opt().map(|value| value.into_str().0)
    }

    fn into_optional_type(self) -> Option<(TypeExpr, Span)> {
        self.into_opt().map(Value::into_type)
    }
}

fn is_lower_camel_case(s: &str) -> bool {
    let bytes = s.as_bytes();
    if bytes.is_empty() {
        return false;
    }
    let start = usize::from(bytes[0] == b'_');
    bytes.get(start).is_some_and(u8::is_ascii_lowercase) && !s[start..].contains('_')
}

fn is_upper_camel_case(s: &str) -> bool {
    let bytes = s.as_bytes();
    if bytes.is_empty() {
        return false;
    }
    let start = usize::from(bytes[0] == b'_');
    bytes.get(start).is_some_and(u8::is_ascii_uppercase)
}

fn is_snake_case(s: &str) -> bool {
    !s.is_empty() && !s.bytes().any(|c| c.is_ascii_uppercase())
}

fn is_valid_namespace_const_name(s: &str) -> bool {
    if s.is_empty() {
        return false;
    }
    if KEYWORD_LIKE_CONSTANTS.contains(&s) {
        return true;
    }
    s.as_bytes()[0] == b'k' && is_upper_camel_case(&s[1..])
}

fn is_valid_type_name(s: &str) -> bool {
    if s.is_empty() {
        return false;
    }
    MACHINE_TYPES.contains(&s) || is_upper_camel_case(s)
}

fn naming_convention_error(context: &mut ActionContext, kind: &str, name: &str, convention: &str) {
    context.lint(format!(
        "{kind} \"{name}\" does not follow \"{convention}\" naming convention."
    ));
}

fn naming_convention_error_at(
    context: &mut ActionContext,
    kind: &str,
    name: &Ident,
    convention: &str,
) {
    context.lint_at(
        format!(
            "{kind} \"{}\" does not follow \"{convention}\" naming convention.",
            name.name
        ),
        name.span,
    );
}

fn lint_generic_parameters(context: &mut ActionContext, parameters: &[GenericParam]) {
    for parameter in parameters {
        if !is_upper_camel_case(&parameter.name.name) {
            naming_convention_error_at(
                context,
                "Generic parameter",
                &parameter.name,
                "UpperCamelCase",
            );
        }
    }
}

fn get_flag(context: &mut ActionContext, name: &str, production: &str) -> bool {
    match BUILD_FLAGS.iter().find(|(flag, _)| *flag == name) {
        Some((_, value)) => *value,
        None => {
            context.report_error(format!(
                "Unknown flag used in {production}: {name}. Please add it to the list in BuildFlags."
            ));
            false
        }
    }
}

struct AnnotationSet {
    set: BTreeSet<String>,
    map: BTreeMap<String, (AnnotationParameter, Span)>,
}

impl AnnotationSet {
    fn new(
        context: &mut ActionContext,
        list: &[AnnotationValue],
        allowed_without_param: &[&str],
        allowed_with_param: &[&str],
    ) -> Self {
        let mut set = BTreeSet::new();
        let mut map = BTreeMap::new();
        for annotation in list {
            let name = annotation.name.name.as_str();
            match &annotation.param {
                Some(param) => {
                    if !allowed_with_param.contains(&name) {
                        let error_message = if allowed_without_param.contains(&name) {
                            " cannot have parameter here"
                        } else {
                            " is not allowed here"
                        };
                        context.lint_at(
                            format!("Annotation {name}{error_message}"),
                            annotation.name.span,
                        );
                    }
                    if map.contains_key(name) {
                        context
                            .lint_at(format!("Duplicate annotation {name}"), annotation.name.span);
                    } else {
                        map.insert(name.to_string(), (param.clone(), annotation.name.span));
                    }
                }
                None => {
                    if !allowed_without_param.contains(&name) {
                        let error_message = if allowed_with_param.contains(&name) {
                            " requires a parameter here"
                        } else {
                            " is not allowed here"
                        };
                        context.lint_at(
                            format!("Annotation {name}{error_message}"),
                            annotation.name.span,
                        );
                    }
                    if !set.insert(name.to_string()) {
                        context
                            .lint_at(format!("Duplicate annotation {name}"), annotation.name.span);
                    }
                }
            }
        }
        AnnotationSet { set, map }
    }

    fn contains(&self, name: &str) -> bool {
        self.set.contains(name)
    }

    fn get_string_param(&self, context: &mut ActionContext, name: &str) -> Option<String> {
        let (param, pos) = self.map.get(name)?;
        if param.is_int {
            context.error_at(
                format!("Annotation {name} requires a string parameter but has an int"),
                *pos,
            );
        }
        Some(param.string_value.clone())
    }

    fn get_int_param(&self, context: &mut ActionContext, name: &str) -> Option<i32> {
        let (param, pos) = self.map.get(name)?;
        if !param.is_int {
            context.error_at(
                format!("Annotation {name} requires an int parameter but has a string"),
                *pos,
            );
        }
        Some(param.int_value)
    }
}

fn process_if_annotation(context: &mut ActionContext, annotations: &AnnotationSet) -> bool {
    if let Some(condition) = annotations.get_string_param(context, ANNOTATION_IF)
        && !get_flag(context, &condition, ANNOTATION_IF)
    {
        return false;
    }
    if let Some(condition) = annotations.get_string_param(context, ANNOTATION_IFNOT)
        && get_flag(context, &condition, ANNOTATION_IFNOT)
    {
        return false;
    }
    true
}

fn process_if_annotation_list(context: &mut ActionContext, list: &[AnnotationValue]) -> bool {
    let annotations = AnnotationSet::new(context, list, &[], &[ANNOTATION_IF, ANNOTATION_IFNOT]);
    process_if_annotation(context, &annotations)
}

fn has_annotation(
    context: &mut ActionContext,
    list: &[AnnotationValue],
    annotation: &str,
    declaration: &str,
) -> bool {
    if list.is_empty() {
        return false;
    }
    if list.len() > 1 || list[0].name.name != annotation {
        context.error(format!(
            "{declaration} declarations only support a single {annotation} annotation"
        ));
    }
    true
}

fn to_ast_annotations(list: &[AnnotationValue]) -> Vec<Annotation> {
    list.iter()
        .map(|annotation| Annotation {
            name: annotation.name.name.clone(),
            argument: annotation.param.as_ref().map(|param| {
                if param.is_int {
                    param.int_value.to_string()
                } else {
                    param.string_value.clone()
                }
            }),
            span: annotation.name.span,
        })
        .collect()
}

fn is_basic_type(ty: &TypeExpr) -> bool {
    matches!(ty, TypeExpr::Basic { .. } | TypeExpr::Reference { .. })
}

fn add_constexpr(context: &mut ActionContext, ty: &TypeExpr) {
    if !is_basic_type(ty) {
        context.report_error("Unsupported extends clause.");
    }
}

enum StatementKind {
    Block { deferred: bool },
    If,
    Other,
}

fn statement_kind(statement: &Stmt) -> StatementKind {
    match statement {
        Stmt::Block { deferred, .. } => StatementKind::Block {
            deferred: *deferred,
        },
        Stmt::Typeswitch { .. } => StatementKind::Block { deferred: false },
        Stmt::Try { body, handlers, .. } if handlers.is_empty() => statement_kind(body),
        Stmt::If { .. } => StatementKind::If,
        _ => StatementKind::Other,
    }
}

fn check_not_deferred_statement(context: &mut ActionContext, statement: &Stmt, pos: Span) {
    if let StatementKind::Block { deferred: true } = statement_kind(statement) {
        context.lint_at(
            "cannot use deferred with a statement block here, it will have no effect",
            pos,
        );
    }
}

fn check_otherwise_labels(context: &mut ActionContext, otherwise: &[(Stmt, Span)]) {
    for (statement, _) in otherwise {
        if let Stmt::Expr(Expr::Ident { generic_args, .. }) = statement
            && !generic_args.is_empty()
        {
            context.report_error("An otherwise label cannot have generic parameters");
        }
    }
}

fn first_token_span(context: &ActionContext, results: &ParseResultIterator) -> Span {
    context.token_span(results.matched().first_token)
}

fn string_literal_unquote(s: &str) -> Option<String> {
    let bytes = s.as_bytes();
    let mut result = Vec::with_capacity(bytes.len());
    let mut i = 1;
    while i + 1 < bytes.len() {
        if bytes[i] == b'\\' {
            i += 1;
            match bytes.get(i) {
                Some(b'n') => result.push(b'\n'),
                Some(b'r') => result.push(b'\r'),
                Some(b't') => result.push(b'\t'),
                Some(c @ (b'\'' | b'"' | b'\\')) => result.push(*c),
                _ => return None,
            }
        } else {
            result.push(bytes[i]);
        }
        i += 1;
    }
    Some(String::from_utf8_lossy(&result).into_owned())
}

fn parse_unsigned_base_zero(text: &str) -> Option<(u128, bool)> {
    let bytes = text.as_bytes();
    let (radix, digits) = if (bytes.starts_with(b"0x") || bytes.starts_with(b"0X"))
        && bytes.get(2).is_some_and(u8::is_ascii_hexdigit)
    {
        (16, &bytes[2..])
    } else if bytes.first() == Some(&b'0') {
        (8, bytes)
    } else {
        (10, bytes)
    };
    let mut value: u128 = 0;
    let mut overflow = false;
    let mut consumed = 0;
    for byte in digits {
        let Some(digit) = char::from(*byte).to_digit(radix) else {
            break;
        };
        consumed += 1;
        value = match value
            .checked_mul(u128::from(radix))
            .and_then(|value| value.checked_add(u128::from(digit)))
        {
            Some(value) if value <= u128::from(u64::MAX) + 1 => value,
            _ => {
                overflow = true;
                u128::from(u64::MAX) + 1
            }
        };
    }
    (consumed > 0).then_some((value, overflow))
}

fn yield_integer_literal(
    context: &mut ActionContext,
    results: &mut ParseResultIterator,
) -> Option<Value> {
    let span = results.span();
    let text = context.text(results.matched().begin, results.matched().end);
    let unsigned = text.strip_prefix('-').unwrap_or(&text);
    match parse_unsigned_base_zero(unsigned) {
        None => context.report_error("integer literal could not be parsed"),
        Some((value, overflow)) if overflow || value > u128::from(u64::MAX) => {
            context.report_error("integer literal value out of range")
        }
        Some(_) => {}
    }
    Some(Value::Str(text, span))
}

fn yield_int32(context: &mut ActionContext, results: &mut ParseResultIterator) -> Option<Value> {
    let text = context.text(results.matched().begin, results.matched().end);
    let (negative, unsigned) = match text.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, text.as_str()),
    };
    match parse_unsigned_base_zero(unsigned) {
        None => {
            context.error("Expected an integer");
            Some(Value::Int32(0))
        }
        Some((magnitude, overflow)) => {
            let value = if negative {
                -(magnitude as i128)
            } else {
                magnitude as i128
            };
            if overflow || value < i128::from(i32::MIN) || value > i128::from(i32::MAX) {
                context.error("Integer out of 32-bit range");
                return Some(Value::Int32(0));
            }
            Some(Value::Int32(value as i32))
        }
    }
}

fn yield_double(context: &mut ActionContext, results: &mut ParseResultIterator) -> Option<Value> {
    let span = results.span();
    let text = context.text(results.matched().begin, results.matched().end);
    let (negative, unsigned) = match text.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, text.as_str()),
    };
    let (mantissa, exponent) = match unsigned.find(['e', 'E']) {
        Some(index) => (&unsigned[..index], &unsigned[index..]),
        None => (unsigned, ""),
    };
    let mut normalized = String::new();
    if negative {
        normalized.push('-');
    }
    if mantissa.starts_with('.') {
        normalized.push('0');
    }
    normalized.push_str(mantissa);
    if mantissa.ends_with('.') {
        normalized.push('0');
    }
    normalized.push_str(exponent);
    let value = normalized.parse::<f64>().unwrap_or(0.0);
    let nonzero_digits = mantissa.bytes().any(|c| c.is_ascii_digit() && c != b'0');
    if value.is_infinite() || value.is_subnormal() || (value == 0.0 && nonzero_digits) {
        context.error("double literal out-of-range");
    }
    Some(Value::Str(text, span))
}

fn default_action(
    _context: &mut ActionContext,
    results: &mut ParseResultIterator,
) -> Option<Value> {
    if !results.has_next() {
        return None;
    }
    Some(results.next())
}

fn cast_to_optional(
    _context: &mut ActionContext,
    results: &mut ParseResultIterator,
) -> Option<Value> {
    Some(Value::Opt(Some(Box::new(results.next()))))
}

fn yield_none(_context: &mut ActionContext, _results: &mut ParseResultIterator) -> Option<Value> {
    Some(Value::Opt(None))
}

fn yield_empty_list(
    _context: &mut ActionContext,
    _results: &mut ParseResultIterator,
) -> Option<Value> {
    Some(Value::List(Vec::new()))
}

fn empty_parameter_list() -> ParamList {
    ParamList {
        implicit_kind: None,
        implicit: Vec::new(),
        named: Vec::new(),
        types_only: Vec::new(),
        rest: None,
        has_varargs: false,
    }
}

fn yield_empty_parameter_list(
    _context: &mut ActionContext,
    _results: &mut ParseResultIterator,
) -> Option<Value> {
    Some(Value::ParameterList(empty_parameter_list(), None))
}

fn yield_true(_context: &mut ActionContext, _results: &mut ParseResultIterator) -> Option<Value> {
    Some(Value::Bool(true))
}

fn yield_false(_context: &mut ActionContext, _results: &mut ParseResultIterator) -> Option<Value> {
    Some(Value::Bool(false))
}

fn make_singleton_vector(
    _context: &mut ActionContext,
    results: &mut ParseResultIterator,
) -> Option<Value> {
    Some(Value::List(vec![results.next()]))
}

fn make_extended_vector(
    _context: &mut ActionContext,
    results: &mut ParseResultIterator,
) -> Option<Value> {
    let mut list = results.next().into_list();
    list.push(results.next());
    Some(Value::List(list))
}

fn conditional_element(enabled: bool, element: Value) -> Value {
    if enabled {
        element
    } else {
        Value::Disabled(Box::new(element))
    }
}

fn make_extended_vector_if_annotation_first(
    context: &mut ActionContext,
    results: &mut ParseResultIterator,
) -> Option<Value> {
    let annotations = results.next().into_annotations();
    let enabled = process_if_annotation_list(context, &annotations);
    Some(Value::List(vec![conditional_element(
        enabled,
        results.next(),
    )]))
}

fn make_extended_vector_if_annotation(
    context: &mut ActionContext,
    results: &mut ParseResultIterator,
) -> Option<Value> {
    let mut list = results.next().into_list();
    let annotations = results.next().into_annotations();
    let enabled = process_if_annotation_list(context, &annotations);
    list.push(conditional_element(enabled, results.next()));
    Some(Value::List(list))
}

fn yield_matched_input(
    context: &mut ActionContext,
    results: &mut ParseResultIterator,
) -> Option<Value> {
    let text = context.text(results.matched().begin, results.matched().end);
    Some(Value::Str(text, results.span()))
}

fn make_identifier(
    _context: &mut ActionContext,
    results: &mut ParseResultIterator,
) -> Option<Value> {
    let (name, _) = results.next().into_str();
    Some(Value::Ident(Ident {
        name,
        span: results.span(),
    }))
}

fn make_identifier_from_matched_input(
    context: &mut ActionContext,
    results: &mut ParseResultIterator,
) -> Option<Value> {
    let name = context.text(results.matched().begin, results.matched().end);
    Some(Value::Ident(Ident {
        name,
        span: results.span(),
    }))
}

fn string_literal_unquote_action(
    context: &mut ActionContext,
    results: &mut ParseResultIterator,
) -> Option<Value> {
    let (literal, span) = results.next().into_str();
    let value = string_literal_unquote(&literal).unwrap_or_else(|| {
        context.report_error("unreachable code");
        String::new()
    });
    Some(Value::Str(value, span))
}

fn make_string_annotation_parameter(
    _context: &mut ActionContext,
    results: &mut ParseResultIterator,
) -> Option<Value> {
    let (value, _) = results.next().into_str();
    Some(Value::AnnotationParameter(AnnotationParameter {
        string_value: value,
        int_value: 0,
        is_int: false,
    }))
}

fn make_int_annotation_parameter(
    _context: &mut ActionContext,
    results: &mut ParseResultIterator,
) -> Option<Value> {
    let value = results.next().into_int32();
    Some(Value::AnnotationParameter(AnnotationParameter {
        string_value: String::new(),
        int_value: value,
        is_int: true,
    }))
}

fn make_annotation(
    _context: &mut ActionContext,
    results: &mut ParseResultIterator,
) -> Option<Value> {
    let name = results.next().into_ident();
    let param = results.next().into_opt().map(|value| match value {
        Value::AnnotationParameter(param) => param,
        other => mismatch("annotation parameter", &other),
    });
    Some(Value::Annotation(AnnotationValue { name, param }))
}

fn make_namespace_qualification(
    _context: &mut ActionContext,
    results: &mut ParseResultIterator,
) -> Option<Value> {
    let global_namespace = results.next().into_bool();
    let mut qualification: Vec<String> = results
        .next()
        .into_list()
        .into_iter()
        .map(|value| value.into_str().0)
        .collect();
    if global_namespace {
        qualification.insert(0, String::new());
    }
    Some(Value::Strings(qualification))
}

fn make_basic_type_expression(
    _context: &mut ActionContext,
    results: &mut ParseResultIterator,
) -> Option<Value> {
    let namespace = results.next().into_strings();
    let is_constexpr = results.next().into_bool();
    let (name, name_span) = results.next().into_str();
    let generic_args = results.next().into_types();
    Some(Value::Type(
        TypeExpr::Basic {
            is_constexpr,
            namespace,
            name: Ident {
                name,
                span: name_span,
            },
            generic_args,
        },
        results.span(),
    ))
}

fn make_function_type_expression(
    context: &mut ActionContext,
    results: &mut ParseResultIterator,
) -> Option<Value> {
    let params = results.next().into_types();
    let (result, _) = results.next().into_type();
    Some(Value::Type(
        TypeExpr::Function {
            params,
            result: Box::new(result),
            span: first_token_span(context, results),
        },
        results.span(),
    ))
}

fn make_reference_type_expression(
    _context: &mut ActionContext,
    results: &mut ParseResultIterator,
) -> Option<Value> {
    let is_const = results.next().into_bool();
    let (inner, _) = results.next().into_type();
    let span = inner.span();
    Some(Value::Type(
        TypeExpr::Reference {
            mutable: !is_const,
            inner: Box::new(inner),
            span,
        },
        results.span(),
    ))
}

fn make_union_type_expression(
    _context: &mut ActionContext,
    results: &mut ParseResultIterator,
) -> Option<Value> {
    let (a, _) = results.next().into_type();
    let (b, _) = results.next().into_type();
    Some(Value::Type(
        TypeExpr::Union(Box::new(a), Box::new(b)),
        results.span(),
    ))
}

fn make_generic_parameter(
    _context: &mut ActionContext,
    results: &mut ParseResultIterator,
) -> Option<Value> {
    let name = results.next().into_ident();
    let constraint = results.next().into_optional_type().map(|(ty, _)| ty);
    Some(Value::GenericParameter(GenericParam {
        name,
        is_variable: true,
        extends: constraint,
    }))
}

fn make_implicit_parameter_list(
    _context: &mut ActionContext,
    results: &mut ParseResultIterator,
) -> Option<Value> {
    let kind = results.next().into_ident();
    let parameters = results
        .next()
        .into_list()
        .into_iter()
        .map(Value::into_name_and_type)
        .collect();
    Some(Value::ImplicitParameters(kind, parameters))
}

fn add_parameter(
    context: &mut ActionContext,
    parameters: &mut Vec<NameAndType>,
    param: NameAndType,
) {
    if !is_lower_camel_case(&param.name.name) {
        naming_convention_error_at(context, "Parameter", &param.name, "lowerCamelCase");
    }
    parameters.push(param);
}

fn make_parameter_list(
    context: &mut ActionContext,
    results: &mut ParseResultIterator,
    has_varargs: bool,
    has_explicit_parameter_names: bool,
) -> Option<Value> {
    let mut list = empty_parameter_list();
    list.has_varargs = has_varargs;
    let mut implicit_kind_pos = None;
    if let Some(Value::ImplicitParameters(kind, parameters)) = results.next().into_opt() {
        implicit_kind_pos = Some(kind.span);
        list.implicit_kind = Some(kind.name);
        for param in parameters {
            add_parameter(context, &mut list.implicit, param);
        }
    }
    if has_explicit_parameter_names {
        let explicit: Vec<NameAndType> = results
            .next()
            .into_list()
            .into_iter()
            .map(Value::into_name_and_type)
            .collect();
        if has_varargs {
            let (name, span) = results.next().into_str();
            list.rest = Some(Ident { name, span });
        }
        for param in explicit {
            add_parameter(context, &mut list.named, param);
        }
    } else {
        list.types_only = results.next().into_types();
    }
    Some(Value::ParameterList(list, implicit_kind_pos))
}

fn make_parameter_list_types(
    context: &mut ActionContext,
    results: &mut ParseResultIterator,
) -> Option<Value> {
    make_parameter_list(context, results, false, false)
}

fn make_parameter_list_varargs_types(
    context: &mut ActionContext,
    results: &mut ParseResultIterator,
) -> Option<Value> {
    make_parameter_list(context, results, true, false)
}

fn make_parameter_list_named(
    context: &mut ActionContext,
    results: &mut ParseResultIterator,
) -> Option<Value> {
    make_parameter_list(context, results, false, true)
}

fn make_parameter_list_named_varargs(
    context: &mut ActionContext,
    results: &mut ParseResultIterator,
) -> Option<Value> {
    make_parameter_list(context, results, true, true)
}

fn make_label_and_types(
    context: &mut ActionContext,
    results: &mut ParseResultIterator,
) -> Option<Value> {
    let name = results.next().into_ident();
    if !is_upper_camel_case(&name.name) {
        naming_convention_error_at(context, "Label", &name, "UpperCamelCase");
    }
    let types = results.next().into_types();
    Some(Value::LabelAndTypes(LabelParam { name, types }))
}

fn deprecated_make_void_type(
    context: &mut ActionContext,
    _results: &mut ParseResultIterator,
) -> Option<Value> {
    context.error("Default void return types are deprecated. Add `: void`.");
    Some(Value::ImplicitVoid)
}

fn make_name_and_type(
    _context: &mut ActionContext,
    results: &mut ParseResultIterator,
) -> Option<Value> {
    let name = results.next().into_ident();
    let (ty, _) = results.next().into_type();
    Some(Value::NameAndType(NameAndType { name, ty }))
}

fn make_class_field(
    context: &mut ActionContext,
    results: &mut ParseResultIterator,
) -> Option<Value> {
    let list = results.next().into_annotations();
    let annotations = AnnotationSet::new(
        context,
        &list,
        &[
            ANNOTATION_CPP_RELAXED_STORE,
            ANNOTATION_CPP_RELAXED_LOAD,
            ANNOTATION_CPP_RELEASE_STORE,
            ANNOTATION_CPP_ACQUIRE_LOAD,
            ANNOTATION_CUSTOM_WEAK_MARKING,
        ],
        &[ANNOTATION_IF, ANNOTATION_IFNOT],
    );
    let synchronization = if annotations.contains(ANNOTATION_CPP_RELEASE_STORE) {
        2
    } else if annotations.contains(ANNOTATION_CPP_RELAXED_STORE) {
        1
    } else {
        0
    };
    let read_synchronization = if annotations.contains(ANNOTATION_CPP_ACQUIRE_LOAD) {
        2
    } else if annotations.contains(ANNOTATION_CPP_RELAXED_LOAD) {
        1
    } else {
        0
    };
    if read_synchronization != synchronization {
        context.error("Incompatible read/write synchronization annotations for a field.");
    }
    let mut conditions = Vec::new();
    if let Some(condition) = annotations.get_string_param(context, ANNOTATION_IF) {
        conditions.push((condition, true));
    }
    if let Some(condition) = annotations.get_string_param(context, ANNOTATION_IFNOT) {
        conditions.push((condition, false));
    }
    let deprecated_weak = results.next().into_bool();
    if deprecated_weak {
        context.error(
            "The keyword 'weak' is deprecated. For a field that can contain a normal weak pointer, \
             use type Weak<T>. For a field that should be marked in some custom way, use \
             @customWeakMarking.",
        );
    }
    let is_const = results.next().into_bool();
    let name = results.next().into_ident();
    let optional = results.next().into_bool();
    let index = results.next().into_opt().map(Value::into_expr);
    if optional && index.is_none() {
        context.error(
            "Fields using optional specifier must also provide an expression indicating the \
             condition for whether the field is present",
        );
    }
    let (ty, _) = results.next().into_type();
    Some(Value::ClassField(
        FieldDecl {
            name,
            ty,
            weak: deprecated_weak,
            is_const,
            index,
            bits: None,
        },
        conditions,
    ))
}

fn make_struct_field(
    _context: &mut ActionContext,
    results: &mut ParseResultIterator,
) -> Option<Value> {
    let is_const = results.next().into_bool();
    let name = results.next().into_ident();
    let (ty, _) = results.next().into_type();
    Some(Value::Field(FieldDecl {
        name,
        ty,
        weak: false,
        is_const,
        index: None,
        bits: None,
    }))
}

fn make_bit_field_declaration(
    _context: &mut ActionContext,
    results: &mut ParseResultIterator,
) -> Option<Value> {
    let name = results.next().into_ident();
    let (ty, _) = results.next().into_type();
    let num_bits = results.next().into_int32();
    Some(Value::Field(FieldDecl {
        name,
        ty,
        weak: false,
        is_const: false,
        index: None,
        bits: Some(num_bits),
    }))
}

fn yield_increment(
    _context: &mut ActionContext,
    results: &mut ParseResultIterator,
) -> Option<Value> {
    Some(Value::IncrementDecrement(true, results.span()))
}

fn yield_decrement(
    _context: &mut ActionContext,
    results: &mut ParseResultIterator,
) -> Option<Value> {
    Some(Value::IncrementDecrement(false, results.span()))
}

fn make_identifier_expression(
    _context: &mut ActionContext,
    results: &mut ParseResultIterator,
) -> Option<Value> {
    let namespace = results.next().into_strings();
    let name = results.next().into_ident();
    let generic_args = results.next().into_types();
    Some(Value::Expr(Expr::Ident {
        namespace,
        name,
        generic_args,
    }))
}

fn make_call(context: &mut ActionContext, results: &mut ParseResultIterator) -> Option<Value> {
    let callee = results.next().into_expr();
    let args = results.next().into_exprs();
    let otherwise = results.next().into_stmts();
    check_otherwise_labels(context, &otherwise);
    let span = callee.span();
    Some(Value::Expr(Expr::Call {
        callee: Box::new(callee),
        args,
        otherwise: otherwise
            .into_iter()
            .map(|(statement, _)| statement)
            .collect(),
        span,
    }))
}

fn make_method_call(
    context: &mut ActionContext,
    results: &mut ParseResultIterator,
) -> Option<Value> {
    let target = results.next().into_expr();
    let method = results.next().into_ident();
    let args = results.next().into_exprs();
    let otherwise = results.next().into_stmts();
    check_otherwise_labels(context, &otherwise);
    let span = target.span().merge(method.span);
    Some(Value::Expr(Expr::MethodCall {
        target: Box::new(target),
        method,
        args,
        otherwise: otherwise
            .into_iter()
            .map(|(statement, _)| statement)
            .collect(),
        span,
    }))
}

fn make_name_and_expression(
    _context: &mut ActionContext,
    results: &mut ParseResultIterator,
) -> Option<Value> {
    let name = results.next().into_ident();
    let expression = results.next().into_expr();
    Some(Value::NameAndExpression(Some(name), expression))
}

fn make_name_and_expression_from_expression(
    context: &mut ActionContext,
    results: &mut ParseResultIterator,
) -> Option<Value> {
    let expression = results.next().into_expr();
    if let Expr::Ident {
        namespace,
        name,
        generic_args,
    } = &expression
    {
        if !generic_args.is_empty() || !namespace.is_empty() {
            context.report_error("expected a plain identifier without qualification");
        }
        let name = name.clone();
        return Some(Value::NameAndExpression(Some(name), expression));
    }
    context.report_error("Constructor parameters need to be named.");
    Some(Value::NameAndExpression(None, expression))
}

fn make_intrinsic_call_expression(
    _context: &mut ActionContext,
    results: &mut ParseResultIterator,
) -> Option<Value> {
    let name = results.next().into_ident();
    let generic_args = results.next().into_types();
    let args = results.next().into_exprs();
    Some(Value::Expr(Expr::IntrinsicCall {
        name,
        generic_args,
        args,
    }))
}

fn make_new_expression(
    context: &mut ActionContext,
    results: &mut ParseResultIterator,
) -> Option<Value> {
    let _pretenured = results.next().into_bool();
    let _clear_padding = results.next().into_bool();
    let (ty, _) = results.next().into_type();
    let fields = results.next().into_named_expressions();
    Some(Value::Expr(Expr::New {
        ty,
        fields,
        span: first_token_span(context, results),
    }))
}

fn make_field_access_expression(
    _context: &mut ActionContext,
    results: &mut ParseResultIterator,
) -> Option<Value> {
    let object = results.next().into_expr();
    let field = results.next().into_ident();
    Some(Value::Expr(Expr::Field {
        object: Box::new(object),
        field,
        via_ref: false,
    }))
}

fn make_reference_field_access_expression(
    _context: &mut ActionContext,
    results: &mut ParseResultIterator,
) -> Option<Value> {
    let object = results.next().into_expr();
    let field = results.next().into_ident();
    Some(Value::Expr(Expr::Field {
        object: Box::new(object),
        field,
        via_ref: true,
    }))
}

fn make_element_access_expression(
    _context: &mut ActionContext,
    results: &mut ParseResultIterator,
) -> Option<Value> {
    let object = results.next().into_expr();
    let index = results.next().into_expr();
    let span = object.span();
    Some(Value::Expr(Expr::Index {
        object: Box::new(object),
        index: Box::new(index),
        span,
    }))
}

fn make_integer_literal_expression(
    _context: &mut ActionContext,
    results: &mut ParseResultIterator,
) -> Option<Value> {
    let (text, span) = results.next().into_str();
    Some(Value::Expr(Expr::Int { text, span }))
}

fn make_floating_point_literal_expression(
    _context: &mut ActionContext,
    results: &mut ParseResultIterator,
) -> Option<Value> {
    let (text, span) = results.next().into_str();
    Some(Value::Expr(Expr::Float { text, span }))
}

fn make_string_literal_expression(
    _context: &mut ActionContext,
    results: &mut ParseResultIterator,
) -> Option<Value> {
    let (literal, span) = results.next().into_str();
    let value = literal[1..literal.len() - 1].to_string();
    Some(Value::Expr(Expr::String { value, span }))
}

fn make_struct_expression(
    _context: &mut ActionContext,
    results: &mut ParseResultIterator,
) -> Option<Value> {
    let (ty, _) = results.next().into_type();
    let fields = results.next().into_named_expressions();
    let span = ty.span();
    Some(Value::Expr(Expr::StructLit { ty, fields, span }))
}

fn call_operator(op: Ident, args: Vec<Expr>, span: Span) -> Expr {
    Expr::Call {
        callee: Box::new(Expr::Ident {
            namespace: Vec::new(),
            name: op,
            generic_args: Vec::new(),
        }),
        args,
        otherwise: Vec::new(),
        span,
    }
}

fn make_unary_operator(
    _context: &mut ActionContext,
    results: &mut ParseResultIterator,
) -> Option<Value> {
    let op = results.next().into_ident();
    let operand = results.next().into_expr();
    let span = op.span.merge(operand.span());
    Some(Value::Expr(call_operator(op, vec![operand], span)))
}

fn make_dereference_expression(
    _context: &mut ActionContext,
    results: &mut ParseResultIterator,
) -> Option<Value> {
    let inner = results.next().into_expr();
    let span = inner.span();
    Some(Value::Expr(Expr::Deref {
        inner: Box::new(inner),
        span,
    }))
}

fn make_spread_expression(
    _context: &mut ActionContext,
    results: &mut ParseResultIterator,
) -> Option<Value> {
    let inner = results.next().into_expr();
    let span = inner.span();
    Some(Value::Expr(Expr::Spread {
        inner: Box::new(inner),
        span,
    }))
}

fn into_increment_decrement(value: Value) -> (bool, Span) {
    match value {
        Value::IncrementDecrement(inc, span) => (inc, span),
        other => mismatch("increment or decrement operator", &other),
    }
}

fn make_increment_decrement_expression_prefix(
    _context: &mut ActionContext,
    results: &mut ParseResultIterator,
) -> Option<Value> {
    let (inc, op_span) = into_increment_decrement(results.next());
    let target = results.next().into_expr();
    let span = op_span.merge(target.span());
    Some(Value::Expr(Expr::IncDec {
        pre: true,
        inc,
        target: Box::new(target),
        span,
    }))
}

fn make_increment_decrement_expression_postfix(
    _context: &mut ActionContext,
    results: &mut ParseResultIterator,
) -> Option<Value> {
    let target = results.next().into_expr();
    let (inc, op_span) = into_increment_decrement(results.next());
    let span = target.span().merge(op_span);
    Some(Value::Expr(Expr::IncDec {
        pre: false,
        inc,
        target: Box::new(target),
        span,
    }))
}

fn make_binary_operator(
    _context: &mut ActionContext,
    results: &mut ParseResultIterator,
) -> Option<Value> {
    let left = results.next().into_expr();
    let op = results.next().into_ident();
    let right = results.next().into_expr();
    let span = left.span().merge(right.span());
    Some(Value::Expr(call_operator(op, vec![left, right], span)))
}

fn make_right_shift_identifier(
    context: &mut ActionContext,
    results: &mut ParseResultIterator,
) -> Option<Value> {
    let text = context.text(results.matched().begin, results.matched().end);
    if text.chars().any(|c| c != '>') {
        context.report_error("right-shift operators may not contain any whitespace");
    }
    Some(Value::Ident(Ident {
        name: text,
        span: results.span(),
    }))
}

fn make_logical_expression(results: &mut ParseResultIterator, op: &str) -> Option<Value> {
    let left = results.next().into_expr();
    let right = results.next().into_expr();
    let span = left.span().merge(right.span());
    Some(Value::Expr(Expr::Logical {
        op: op.into(),
        left: Box::new(left),
        right: Box::new(right),
        span,
    }))
}

fn make_logical_and_expression(
    _context: &mut ActionContext,
    results: &mut ParseResultIterator,
) -> Option<Value> {
    make_logical_expression(results, "&&")
}

fn make_logical_or_expression(
    _context: &mut ActionContext,
    results: &mut ParseResultIterator,
) -> Option<Value> {
    make_logical_expression(results, "||")
}

fn make_conditional_expression(
    _context: &mut ActionContext,
    results: &mut ParseResultIterator,
) -> Option<Value> {
    let cond = results.next().into_expr();
    let then_e = results.next().into_expr();
    let else_e = results.next().into_expr();
    let span = cond.span().merge(else_e.span());
    Some(Value::Expr(Expr::Conditional {
        cond: Box::new(cond),
        then_e: Box::new(then_e),
        else_e: Box::new(else_e),
        span,
    }))
}

fn extract_assignment_operator(
    _context: &mut ActionContext,
    results: &mut ParseResultIterator,
) -> Option<Value> {
    let op = results.next().into_ident();
    let operator = op.name[..op.name.len() - 1].to_string();
    Some(Value::Opt(Some(Box::new(Value::Str(operator, op.span)))))
}

fn make_assignment_expression(
    _context: &mut ActionContext,
    results: &mut ParseResultIterator,
) -> Option<Value> {
    let target = results.next().into_expr();
    let op = results.next().into_optional_string();
    let value = results.next().into_expr();
    let span = target.span().merge(value.span());
    Some(Value::Expr(Expr::Assign {
        target: Box::new(target),
        op,
        value: Box::new(value),
        span,
    }))
}

fn make_block_statement(
    context: &mut ActionContext,
    results: &mut ParseResultIterator,
) -> Option<Value> {
    let deferred = results.next().into_bool();
    let statements: Vec<(Stmt, Span, bool)> = results
        .next()
        .into_list()
        .into_iter()
        .map(|value| {
            let (value, enabled) = split_enabled(value);
            let (statement, pos) = value.into_stmt();
            (statement, pos, enabled)
        })
        .collect();
    for (statement, pos, enabled) in &statements {
        if *enabled {
            check_not_deferred_statement(context, statement, *pos);
        }
    }
    let matched = results.matched();
    let open = context.token_span(matched.first_token + usize::from(deferred));
    let close = context.token_span(matched.end_token - 1);
    Some(Value::Stmt(
        Stmt::Block {
            deferred,
            statements: statements
                .into_iter()
                .map(|(statement, _, _)| statement)
                .collect(),
            span: open.merge(close),
        },
        results.span(),
    ))
}

fn into_try_handler(value: Value) -> (TryHandler, Span) {
    match value {
        Value::TryHandler(handler, pos) => (handler, pos),
        other => mismatch("try handler", &other),
    }
}

fn make_label_block(
    context: &mut ActionContext,
    results: &mut ParseResultIterator,
) -> Option<Value> {
    let name = results.next().into_ident();
    if !is_upper_camel_case(&name.name) {
        naming_convention_error_at(context, "Label", &name, "UpperCamelCase");
    }
    let (params, _) = results.next().into_parameter_list();
    let (body, _) = results.next().into_stmt();
    Some(Value::TryHandler(
        TryHandler::Label {
            name,
            params,
            body: Box::new(body),
        },
        results.span(),
    ))
}

fn make_catch_block(
    context: &mut ActionContext,
    results: &mut ParseResultIterator,
) -> Option<Value> {
    let parameter_names: Vec<(String, Span)> = results
        .next()
        .into_list()
        .into_iter()
        .map(Value::into_str)
        .collect();
    let (body, _) = results.next().into_stmt();
    for (variable, _) in &parameter_names {
        if !is_lower_camel_case(variable) {
            naming_convention_error(context, "Exception", variable, "lowerCamelCase");
        }
    }
    if parameter_names.len() != 2 {
        context.report_error(
            "A catch clause needs to have exactly two parameters: The exception and the message. \
             How about: \"catch (exception, message) { ...\".",
        );
    }
    Some(Value::TryHandler(
        TryHandler::Catch {
            names: parameter_names
                .into_iter()
                .map(|(name, span)| Ident { name, span })
                .collect(),
            body: Box::new(body),
        },
        results.span(),
    ))
}

fn make_expression_with_source(
    _context: &mut ActionContext,
    results: &mut ParseResultIterator,
) -> Option<Value> {
    Some(Value::Expr(results.next().into_expr()))
}

fn make_enum_entry(
    context: &mut ActionContext,
    results: &mut ParseResultIterator,
) -> Option<Value> {
    let list = results.next().into_annotations();
    let annotations = AnnotationSet::new(context, &list, &[], &[ANNOTATION_SAME_ENUM_VALUE_AS]);
    let _alias_entry = annotations.get_string_param(context, ANNOTATION_SAME_ENUM_VALUE_AS);
    let name = results.next().into_ident();
    let ty = results.next().into_optional_type();
    Some(Value::EnumEntry(EnumEntryValue { name, ty }))
}

fn make_var_declaration_statement(
    context: &mut ActionContext,
    results: &mut ParseResultIterator,
) -> Option<Value> {
    let kind = results.next().into_ident();
    let is_const = kind.name == "const";
    let name = results.next().into_ident();
    if !is_lower_camel_case(&name.name) {
        naming_convention_error_at(context, "Variable", &name, "lowerCamelCase");
    }
    let ty = results.next().into_optional_type().map(|(ty, _)| ty);
    let init = results.has_next().then(|| results.next().into_expr());
    if init.is_none() && ty.is_none() {
        context.report_error("Declaration is missing a type.");
    }
    Some(Value::Stmt(
        Stmt::Var {
            is_const,
            name,
            ty,
            init,
            span: kind.span,
        },
        results.span(),
    ))
}

fn make_expression_statement(
    _context: &mut ActionContext,
    results: &mut ParseResultIterator,
) -> Option<Value> {
    let expression = results.next().into_expr();
    Some(Value::Stmt(Stmt::Expr(expression), results.span()))
}

fn make_return_statement(
    context: &mut ActionContext,
    results: &mut ParseResultIterator,
) -> Option<Value> {
    let value = results.next().into_opt().map(Value::into_expr);
    Some(Value::Stmt(
        Stmt::Return {
            value,
            span: first_token_span(context, results),
        },
        results.span(),
    ))
}

fn make_tail_call_statement(
    _context: &mut ActionContext,
    results: &mut ParseResultIterator,
) -> Option<Value> {
    let call = results.next().into_expr();
    Some(Value::Stmt(Stmt::TailCall(call), results.span()))
}

fn make_break_statement(
    context: &mut ActionContext,
    results: &mut ParseResultIterator,
) -> Option<Value> {
    Some(Value::Stmt(
        Stmt::Break {
            span: first_token_span(context, results),
        },
        results.span(),
    ))
}

fn make_continue_statement(
    context: &mut ActionContext,
    results: &mut ParseResultIterator,
) -> Option<Value> {
    Some(Value::Stmt(
        Stmt::Continue {
            span: first_token_span(context, results),
        },
        results.span(),
    ))
}

fn make_goto_statement(
    _context: &mut ActionContext,
    results: &mut ParseResultIterator,
) -> Option<Value> {
    let label = results.next().into_ident();
    let args = results.next().into_exprs();
    Some(Value::Stmt(Stmt::Goto { label, args }, results.span()))
}

fn make_debug_statement(
    _context: &mut ActionContext,
    results: &mut ParseResultIterator,
) -> Option<Value> {
    let kind = results.next().into_ident();
    Some(Value::Stmt(
        Stmt::Debug {
            kind: kind.name,
            span: kind.span,
        },
        results.span(),
    ))
}

fn make_if_statement(
    context: &mut ActionContext,
    results: &mut ParseResultIterator,
) -> Option<Value> {
    let is_constexpr = results.next().into_bool();
    let cond = results.next().into_expr();
    let (if_true, if_true_pos) = results.next().into_stmt();
    let if_false = results.next().into_opt().map(Value::into_stmt);
    if let Some((if_false, _)) = &if_false
        && !(matches!(statement_kind(&if_true), StatementKind::Block { .. })
            && matches!(
                statement_kind(if_false),
                StatementKind::Block { .. } | StatementKind::If
            ))
    {
        context.report_error("if-else statements require curly braces");
    }
    if is_constexpr {
        check_not_deferred_statement(context, &if_true, if_true_pos);
        if let Some((if_false, if_false_pos)) = &if_false {
            check_not_deferred_statement(context, if_false, *if_false_pos);
        }
    }
    Some(Value::Stmt(
        Stmt::If {
            is_constexpr,
            cond,
            then_s: Box::new(if_true),
            else_s: if_false.map(|(statement, _)| Box::new(statement)),
            span: first_token_span(context, results),
        },
        results.span(),
    ))
}

fn make_typeswitch_statement(
    context: &mut ActionContext,
    results: &mut ParseResultIterator,
) -> Option<Value> {
    let expr = results.next().into_expr();
    let cases = results
        .next()
        .into_list()
        .into_iter()
        .map(|value| match split_enabled(value).0 {
            Value::TypeswitchCase(case) => case,
            other => mismatch("typeswitch case", &other),
        })
        .collect();
    Some(Value::Stmt(
        Stmt::Typeswitch {
            expr,
            cases,
            span: first_token_span(context, results),
        },
        results.span(),
    ))
}

fn make_typeswitch_case(
    _context: &mut ActionContext,
    results: &mut ParseResultIterator,
) -> Option<Value> {
    let name = results.next().into_opt().map(Value::into_ident);
    let (ty, _) = results.next().into_type();
    let (body, _) = results.next().into_stmt();
    Some(Value::TypeswitchCase(TypeswitchCase {
        name,
        ty,
        body: Box::new(body),
    }))
}

fn make_try_label_expression(
    context: &mut ActionContext,
    results: &mut ParseResultIterator,
) -> Option<Value> {
    let (try_block, try_block_pos) = results.next().into_stmt();
    check_not_deferred_statement(context, &try_block, try_block_pos);
    let handlers: Vec<(TryHandler, Span)> = results
        .next()
        .into_list()
        .into_iter()
        .map(into_try_handler)
        .collect();
    if handlers.is_empty() {
        context.error("Try blocks without catch or label don't make sense.");
    }
    for (index, (handler, pos)) in handlers.iter().enumerate() {
        if index != 0 && matches!(handler, TryHandler::Catch { .. }) {
            context.error_at(
                "A catch handler always has to be first, before any label handler, to avoid \
                 ambiguity about whether it catches exceptions from preceding handlers or not.",
                *pos,
            );
        }
    }
    let pos = if handlers.is_empty() {
        try_block_pos
    } else {
        results.span()
    };
    Some(Value::Stmt(
        Stmt::Try {
            body: Box::new(try_block),
            handlers: handlers.into_iter().map(|(handler, _)| handler).collect(),
            span: first_token_span(context, results),
        },
        pos,
    ))
}

fn make_assert_statement(
    _context: &mut ActionContext,
    results: &mut ParseResultIterator,
) -> Option<Value> {
    let kind = results.next().into_ident();
    let expr = results.next().into_expr();
    Some(Value::Stmt(
        Stmt::Assert {
            kind: kind.name,
            expr,
            span: kind.span,
        },
        results.span(),
    ))
}

fn make_while_statement(
    context: &mut ActionContext,
    results: &mut ParseResultIterator,
) -> Option<Value> {
    let cond = results.next().into_expr();
    let (body, _) = results.next().into_stmt();
    Some(Value::Stmt(
        Stmt::While {
            cond,
            body: Box::new(body),
            span: first_token_span(context, results),
        },
        results.span(),
    ))
}

fn make_for_loop_statement(
    context: &mut ActionContext,
    results: &mut ParseResultIterator,
) -> Option<Value> {
    let var_decl = results.next().into_optional_stmt();
    let test = results.next().into_opt().map(Value::into_expr);
    let action = results.next().into_opt().map(Value::into_expr);
    let (body, body_pos) = results.next().into_stmt();
    check_not_deferred_statement(context, &body, body_pos);
    Some(Value::Stmt(
        Stmt::For {
            init: var_decl.map(Box::new),
            cond: test,
            step: action,
            body: Box::new(body),
            span: first_token_span(context, results),
        },
        results.span(),
    ))
}

fn check_macro_parameters(
    context: &mut ActionContext,
    params: &ParamList,
    implicit_kind_pos: Option<Span>,
) {
    if params.implicit_kind.as_deref() == Some("js-implicit")
        && let Some(pos) = implicit_kind_pos
    {
        context.error_at(
            "Cannot use \"js-implicit\" with macros, use \"implicit\" instead.",
            pos,
        );
    }
}

fn check_intrinsic_parameters(context: &mut ActionContext, params: &ParamList) {
    if params.implicit_kind.is_some() {
        context.error("Intinsics cannot have implicit parameters.");
    }
}

fn check_builtin_parameters(
    context: &mut ActionContext,
    params: &ParamList,
    implicit_kind_pos: Option<Span>,
    javascript_linkage: bool,
) {
    match params.implicit_kind.as_deref() {
        Some("js-implicit") if !javascript_linkage => context.error(
            "\"js-implicit\" is for implicit parameters passed according to the JavaScript \
             calling convention. Use \"implicit\" instead.",
        ),
        Some("implicit") if javascript_linkage => {
            if let Some(pos) = implicit_kind_pos {
                context.error_at(
                    "The JavaScript calling convention implicitly passes a fixed set of values. \
                     Use \"js-implicit\" to refer to those.",
                    pos,
                );
            }
        }
        _ => {}
    }
}

fn make_method_declaration(
    context: &mut ActionContext,
    results: &mut ParseResultIterator,
) -> Option<Value> {
    let transitioning = results.next().into_bool();
    let operator_name = results.next().into_optional_string();
    let name = results.next().into_ident();
    if !is_upper_camel_case(&name.name) {
        naming_convention_error_at(context, "Method", &name, "UpperCamelCase");
    }
    let (params, implicit_kind_pos) = results.next().into_parameter_list();
    let return_type = results.next().into_return_type();
    let labels = results.next().into_labels();
    let (body, _) = results.next().into_stmt();
    check_macro_parameters(context, &params, implicit_kind_pos);
    Some(Value::Method(CallableDecl {
        annotations: Vec::new(),
        transitioning,
        javascript: false,
        is_extern: false,
        operator_name,
        kind: CallableKind::Macro,
        name,
        generic_params: Vec::new(),
        params,
        return_type,
        labels,
        body: Some(Box::new(body)),
    }))
}

fn make_class_body(
    _context: &mut ActionContext,
    results: &mut ParseResultIterator,
) -> Option<Value> {
    let methods = results
        .next()
        .into_list()
        .into_iter()
        .map(Value::into_method)
        .collect();
    let fields = results
        .next()
        .into_list()
        .into_iter()
        .map(|value| match value {
            Value::ClassField(field, conditions) => (field, conditions),
            other => mismatch("class field", &other),
        })
        .collect();
    Some(Value::Opt(Some(Box::new(Value::ClassBody(
        methods, fields,
    )))))
}

fn make_const_declaration(
    context: &mut ActionContext,
    results: &mut ParseResultIterator,
) -> Option<Value> {
    let annotations = results.next().into_annotations();
    process_if_annotation_list(context, &annotations);
    let name = results.next().into_ident();
    if !is_valid_namespace_const_name(&name.name) {
        naming_convention_error_at(context, "Constant", &name, "kUpperCamelCase");
    }
    let (ty, _) = results.next().into_type();
    let expression = results.next().into_expr();
    Some(Value::Decls(vec![Decl::Const {
        name,
        ty,
        init: Some(expression),
        generates: None,
        annotations: to_ast_annotations(&annotations),
    }]))
}

fn make_extern_const_declaration(
    _context: &mut ActionContext,
    results: &mut ParseResultIterator,
) -> Option<Value> {
    let name = results.next().into_ident();
    let (ty, _) = results.next().into_type();
    let (literal, _) = results.next().into_str();
    Some(Value::Decls(vec![Decl::Const {
        name,
        ty,
        init: None,
        generates: Some(literal),
        annotations: Vec::new(),
    }]))
}

fn make_class_declaration(
    context: &mut ActionContext,
    results: &mut ParseResultIterator,
) -> Option<Value> {
    let list = results.next().into_annotations();
    let annotations = AnnotationSet::new(
        context,
        &list,
        &[
            ANNOTATION_ABSTRACT,
            ANNOTATION_HAS_SAME_INSTANCE_TYPE_AS_PARENT,
            ANNOTATION_DO_NOT_GENERATE_CPP_CLASS,
            ANNOTATION_CUSTOM_CPP_CLASS,
            ANNOTATION_CUSTOM_MAP,
            ANNOTATION_EXPORT,
            ANNOTATION_DO_NOT_GENERATE_CAST,
            ANNOTATION_DO_NOT_GENERATE_INSTANCE_TYPE_CHECK,
            ANNOTATION_HIGHEST_INSTANCE_TYPE_WITHIN_PARENT,
            ANNOTATION_LOWEST_INSTANCE_TYPE_WITHIN_PARENT,
            ANNOTATION_CPP_OBJECT_LAYOUT_DEFINITION,
        ],
        &[
            ANNOTATION_RESERVE_BITS_IN_INSTANCE_TYPE,
            ANNOTATION_INSTANCE_TYPE_VALUE,
        ],
    );
    let do_not_generate_cpp_class = annotations.contains(ANNOTATION_DO_NOT_GENERATE_CPP_CLASS);
    if annotations.contains(ANNOTATION_CUSTOM_CPP_CLASS) {
        context.error(
            "@customCppClass is deprecated. Use 'extern' instead. @generateUniqueMap accomplishes \
             most of what '@export @customCppClass' used to.",
        );
    }
    if annotations.contains(ANNOTATION_CUSTOM_MAP) {
        context.error(
            "@customMap is deprecated. Generating a unique map is opt-in now using \
             @generateUniqueMap.",
        );
    }
    let is_extern = results.next().into_bool();
    let transient = results.next().into_bool();
    let kind = results.next().into_ident();
    let is_shape = kind.name == "shape";
    let name = results.next().into_ident();
    if !is_valid_type_name(&name.name) {
        naming_convention_error_at(context, "Type", &name, "UpperCamelCase");
    }
    let (extends, _) = results.next().into_type();
    if !is_basic_type(&extends) {
        context.report_error("Expected type name in extends clause.");
    }
    let generates = results.next().into_optional_string();
    let body = results.next().into_opt();
    let has_body = body.is_some();
    let (methods, fields) = match body {
        Some(Value::ClassBody(methods, fields)) => (methods, fields),
        Some(other) => mismatch("class body", &other),
        None => (Vec::new(), Vec::new()),
    };
    if !(is_extern && has_body && !is_shape) && do_not_generate_cpp_class {
        context.lint("Annotation @doNotGenerateCppClass has no effect");
    }
    for (_, conditions) in &fields {
        for (condition, positive) in conditions {
            let excluded = if *positive {
                !get_flag(context, condition, ANNOTATION_IF)
            } else {
                get_flag(context, condition, ANNOTATION_IFNOT)
            };
            if excluded {
                break;
            }
        }
    }
    annotations.get_int_param(context, ANNOTATION_INSTANCE_TYPE_VALUE);
    annotations.get_int_param(context, ANNOTATION_RESERVE_BITS_IN_INSTANCE_TYPE);
    Some(Value::Decls(vec![Decl::Class {
        name,
        is_extern,
        is_shape,
        transient,
        extends: Some(extends),
        generates,
        fields: fields.into_iter().map(|(field, _)| field).collect(),
        methods,
        annotations: to_ast_annotations(&list),
        span: kind.span,
    }]))
}

fn make_struct_declaration(
    context: &mut ActionContext,
    results: &mut ParseResultIterator,
) -> Option<Value> {
    let list = results.next().into_annotations();
    has_annotation(context, &list, ANNOTATION_EXPORT, "Struct");
    let name = results.next().into_ident();
    if !is_valid_type_name(&name.name) {
        naming_convention_error_at(context, "Struct", &name, "UpperCamelCase");
    }
    let generic_params = results.next().into_generic_parameters();
    lint_generic_parameters(context, &generic_params);
    let methods = results
        .next()
        .into_list()
        .into_iter()
        .map(|value| split_enabled(value).0.into_method())
        .collect();
    let fields = results
        .next()
        .into_list()
        .into_iter()
        .map(|value| match split_enabled(value).0 {
            Value::Field(field) => field,
            other => mismatch("struct field", &other),
        })
        .collect();
    Some(Value::Decls(vec![Decl::Struct {
        name,
        generic_params,
        fields,
        methods,
        annotations: to_ast_annotations(&list),
    }]))
}

fn make_bit_field_struct_declaration(
    context: &mut ActionContext,
    results: &mut ParseResultIterator,
) -> Option<Value> {
    let list = results.next().into_annotations();
    let annotations = AnnotationSet::new(context, &list, &[], &[ANNOTATION_CPP_SCOPE]);
    annotations.get_string_param(context, ANNOTATION_CPP_SCOPE);
    let name = results.next().into_ident();
    if !is_valid_type_name(&name.name) {
        naming_convention_error_at(context, "Bitfield struct", &name, "UpperCamelCase");
    }
    let (extends, _) = results.next().into_type();
    let fields = results
        .next()
        .into_list()
        .into_iter()
        .map(|value| match split_enabled(value).0 {
            Value::Field(field) => field,
            other => mismatch("bitfield", &other),
        })
        .collect();
    Some(Value::Decls(vec![Decl::BitFieldStruct {
        name,
        extends,
        fields,
        annotations: to_ast_annotations(&list),
    }]))
}

fn make_abstract_type_declaration(
    context: &mut ActionContext,
    results: &mut ParseResultIterator,
) -> Option<Value> {
    let list = results.next().into_annotations();
    has_annotation(
        context,
        &list,
        ANNOTATION_USE_PARENT_TYPE_CHECKER,
        "abstract type",
    );
    let transient = results.next().into_bool();
    let name = results.next().into_ident();
    if !is_valid_type_name(&name.name) {
        naming_convention_error_at(context, "Type", &name, "UpperCamelCase");
    }
    let generic_params = results.next().into_generic_parameters();
    let extends = results.next().into_optional_type().map(|(ty, _)| ty);
    let generates = results.next().into_optional_string();
    let constexpr_generates = results.next().into_optional_string();
    if constexpr_generates.is_some()
        && let Some(extends) = &extends
    {
        add_constexpr(context, extends);
    }
    Some(Value::Decls(vec![Decl::AbstractType {
        name,
        generic_params,
        extends,
        generates,
        constexpr_generates,
        transient,
        annotations: to_ast_annotations(&list),
    }]))
}

fn make_type_alias_declaration(
    context: &mut ActionContext,
    results: &mut ParseResultIterator,
) -> Option<Value> {
    let list = results.next().into_annotations();
    process_if_annotation_list(context, &list);
    let name = results.next().into_ident();
    let (ty, _) = results.next().into_type();
    Some(Value::Decls(vec![Decl::TypeAlias {
        name,
        generic_params: Vec::new(),
        ty,
        annotations: to_ast_annotations(&list),
    }]))
}

fn make_intrinsic_declaration(
    context: &mut ActionContext,
    results: &mut ParseResultIterator,
) -> Option<Value> {
    let name = results.next().into_ident();
    let generic_params = results.next().into_generic_parameters();
    lint_generic_parameters(context, &generic_params);
    let (params, implicit_kind_pos) = results.next().into_parameter_list();
    let return_type = results.next().into_return_type();
    let body = results.next().into_optional_stmt();
    if body.is_some() {
        check_macro_parameters(context, &params, implicit_kind_pos);
    } else {
        check_intrinsic_parameters(context, &params);
    }
    Some(Value::Decls(vec![Decl::Callable(CallableDecl {
        annotations: Vec::new(),
        transitioning: false,
        javascript: false,
        is_extern: false,
        operator_name: None,
        kind: CallableKind::Intrinsic,
        name,
        generic_params,
        params,
        return_type,
        labels: Vec::new(),
        body: body.map(Box::new),
    })]))
}

fn make_external_macro(
    context: &mut ActionContext,
    results: &mut ParseResultIterator,
) -> Option<Value> {
    let list = results.next().into_annotations();
    let enabled = process_if_annotation_list(context, &list);
    let transitioning = results.next().into_bool();
    let operator_name = results.next().into_optional_string();
    let _external_assembler_name = results.next().into_optional_string();
    let name = results.next().into_ident();
    let generic_params = results.next().into_generic_parameters();
    lint_generic_parameters(context, &generic_params);
    let (params, implicit_kind_pos) = results.next().into_parameter_list();
    let return_type = results.next().into_return_type();
    let labels = results.next().into_labels();
    if enabled {
        check_macro_parameters(context, &params, implicit_kind_pos);
    }
    if !generic_params.is_empty() {
        context.error("External builtins cannot be generic.");
    }
    Some(Value::Decls(vec![Decl::Callable(CallableDecl {
        annotations: to_ast_annotations(&list),
        transitioning,
        javascript: false,
        is_extern: true,
        operator_name,
        kind: CallableKind::Macro,
        name,
        generic_params,
        params,
        return_type,
        labels,
        body: None,
    })]))
}

fn make_external_builtin(
    context: &mut ActionContext,
    results: &mut ParseResultIterator,
) -> Option<Value> {
    let list = results.next().into_annotations();
    process_if_annotation_list(context, &list);
    let transitioning = results.next().into_bool();
    let javascript = results.next().into_bool();
    let name = results.next().into_ident();
    let generic_params = results.next().into_generic_parameters();
    lint_generic_parameters(context, &generic_params);
    let (params, implicit_kind_pos) = results.next().into_parameter_list();
    let return_type = results.next().into_return_type();
    check_builtin_parameters(context, &params, implicit_kind_pos, javascript);
    if !generic_params.is_empty() {
        context.error("External builtins cannot be generic.");
    }
    Some(Value::Decls(vec![Decl::Callable(CallableDecl {
        annotations: to_ast_annotations(&list),
        transitioning,
        javascript,
        is_extern: true,
        operator_name: None,
        kind: CallableKind::Builtin,
        name,
        generic_params,
        params,
        return_type,
        labels: Vec::new(),
        body: None,
    })]))
}

fn make_external_runtime(
    _context: &mut ActionContext,
    results: &mut ParseResultIterator,
) -> Option<Value> {
    let transitioning = results.next().into_bool();
    let name = results.next().into_ident();
    let (params, _) = results.next().into_parameter_list();
    let return_type = results.next().into_return_type();
    Some(Value::Decls(vec![Decl::Callable(CallableDecl {
        annotations: Vec::new(),
        transitioning,
        javascript: false,
        is_extern: true,
        operator_name: None,
        kind: CallableKind::Runtime,
        name,
        generic_params: Vec::new(),
        params,
        return_type,
        labels: Vec::new(),
        body: None,
    })]))
}

fn make_torque_macro_declaration(
    context: &mut ActionContext,
    results: &mut ParseResultIterator,
) -> Option<Value> {
    let list = results.next().into_annotations();
    let annotations = AnnotationSet::new(
        context,
        &list,
        &[ANNOTATION_EXPORT, ANNOTATION_SUPPORTS_TSA],
        &[ANNOTATION_IF, ANNOTATION_IFNOT],
    );
    let enabled = process_if_annotation(context, &annotations);
    let export_to_csa = annotations.contains(ANNOTATION_EXPORT);
    let transitioning = results.next().into_bool();
    let operator_name = results.next().into_optional_string();
    let name = results.next().into_ident();
    if !is_upper_camel_case(&name.name) {
        naming_convention_error_at(context, "Macro", &name, "UpperCamelCase");
    }
    let generic_params = results.next().into_generic_parameters();
    lint_generic_parameters(context, &generic_params);
    let (params, implicit_kind_pos) = results.next().into_parameter_list();
    let return_type = results.next().into_return_type();
    let labels = results.next().into_labels();
    let body = results.next().into_optional_stmt();
    if enabled {
        check_macro_parameters(context, &params, implicit_kind_pos);
        if generic_params.is_empty() {
            if body.is_none() {
                context.report_error("A non-generic declaration needs a body.");
            }
        } else if export_to_csa {
            context.report_error("Cannot export generics to CSA.");
        }
    }
    Some(Value::Decls(vec![Decl::Callable(CallableDecl {
        annotations: to_ast_annotations(&list),
        transitioning,
        javascript: false,
        is_extern: false,
        operator_name,
        kind: CallableKind::Macro,
        name,
        generic_params,
        params,
        return_type,
        labels,
        body: body.map(Box::new),
    })]))
}

fn make_torque_builtin_declaration(
    context: &mut ActionContext,
    results: &mut ParseResultIterator,
) -> Option<Value> {
    let list = results.next().into_annotations();
    let annotations = AnnotationSet::new(
        context,
        &list,
        &[
            ANNOTATION_CUSTOM_INTERFACE_DESCRIPTOR,
            ANNOTATION_SUPPORTS_TSA,
        ],
        &[
            ANNOTATION_IF,
            ANNOTATION_IFNOT,
            ANNOTATION_INCREMENT_USE_COUNTER,
        ],
    );
    let use_counter_name = annotations.get_string_param(context, ANNOTATION_INCREMENT_USE_COUNTER);
    process_if_annotation(context, &annotations);
    let transitioning = results.next().into_bool();
    let javascript = results.next().into_bool();
    let name = results.next().into_ident();
    if !is_upper_camel_case(&name.name) {
        naming_convention_error_at(context, "Builtin", &name, "UpperCamelCase");
    }
    let generic_params = results.next().into_generic_parameters();
    lint_generic_parameters(context, &generic_params);
    let (params, implicit_kind_pos) = results.next().into_parameter_list();
    let return_type = results.next().into_return_type();
    let body = results.next().into_optional_stmt();
    check_builtin_parameters(context, &params, implicit_kind_pos, javascript);
    if generic_params.is_empty() && body.is_none() {
        context.report_error("A non-generic declaration needs a body.");
    }
    if use_counter_name.is_some() && body.is_none() {
        context.report_error("@incrementUseCounter needs a body.");
    }
    Some(Value::Decls(vec![Decl::Callable(CallableDecl {
        annotations: to_ast_annotations(&list),
        transitioning,
        javascript,
        is_extern: false,
        operator_name: None,
        kind: CallableKind::Builtin,
        name,
        generic_params,
        params,
        return_type,
        labels: Vec::new(),
        body: body.map(Box::new),
    })]))
}

fn type_expr_name(ty: &TypeExpr) -> String {
    match ty {
        TypeExpr::Basic {
            is_constexpr,
            namespace,
            name,
            generic_args,
        } => {
            let mut text = String::new();
            if *is_constexpr {
                text.push_str("constexpr ");
            }
            if !namespace.is_empty() {
                text.push_str(&namespace.join("::"));
                text.push_str("::");
            }
            text.push_str(&name.name);
            if !generic_args.is_empty() {
                text.push('<');
                text.push_str(
                    &generic_args
                        .iter()
                        .map(type_expr_name)
                        .collect::<Vec<_>>()
                        .join(", "),
                );
                text.push('>');
            }
            text
        }
        TypeExpr::Union(left, right) => {
            format!("{}|{}", type_expr_name(left), type_expr_name(right))
        }
        TypeExpr::Reference { mutable, inner, .. } => {
            if *mutable {
                format!("&{}", type_expr_name(inner))
            } else {
                format!("const &{}", type_expr_name(inner))
            }
        }
        TypeExpr::Function { .. } => "builtin".into(),
    }
}

fn type_expr_as_ident(ty: &TypeExpr) -> Ident {
    match ty {
        TypeExpr::Basic {
            is_constexpr,
            namespace,
            name,
            generic_args,
        } if generic_args.is_empty() && namespace.is_empty() => {
            if *is_constexpr {
                Ident {
                    name: format!("constexpr {}", name.name),
                    span: name.span,
                }
            } else {
                name.clone()
            }
        }
        _ => Ident {
            name: type_expr_name(ty),
            span: ty.span(),
        },
    }
}

fn make_specialization_declaration(
    context: &mut ActionContext,
    results: &mut ParseResultIterator,
) -> Option<Value> {
    let list = results.next().into_annotations();
    process_if_annotation_list(context, &list);
    let transitioning = results.next().into_bool();
    let name = results.next().into_ident();
    let generic_params = results
        .next()
        .into_types()
        .iter()
        .map(|ty| GenericParam {
            name: type_expr_as_ident(ty),
            is_variable: false,
            extends: None,
        })
        .collect();
    let (params, _) = results.next().into_parameter_list();
    let return_type = results.next().into_return_type();
    let labels = results.next().into_labels();
    let (body, body_pos) = results.next().into_stmt();
    check_not_deferred_statement(context, &body, body_pos);
    Some(Value::Decls(vec![Decl::Callable(CallableDecl {
        annotations: to_ast_annotations(&list),
        transitioning,
        javascript: false,
        is_extern: false,
        operator_name: None,
        kind: CallableKind::Macro,
        name,
        generic_params,
        params,
        return_type,
        labels,
        body: Some(Box::new(body)),
    })]))
}

fn make_cpp_include_declaration(
    context: &mut ActionContext,
    results: &mut ParseResultIterator,
) -> Option<Value> {
    let include_selector = results.next().into_optional_string();
    let (path, span) = results.next().into_str();
    if let Some(selector) = include_selector
        && selector != "csa"
        && selector != "tsa"
    {
        context.error(format!("'{selector}' is not a valid include selector"));
    }
    Some(Value::Decls(vec![Decl::Include { path, span }]))
}

fn make_enum_declaration(
    context: &mut ActionContext,
    results: &mut ParseResultIterator,
) -> Option<Value> {
    let is_extern = results.next().into_bool();
    let name = results.next().into_ident();
    let base_type_expression = results.next().into_optional_type();
    let constexpr_generates = results.next().into_optional_string();
    let entries: Vec<(EnumEntryValue, bool)> = results
        .next()
        .into_list()
        .into_iter()
        .map(|value| match split_enabled(value) {
            (Value::EnumEntry(entry), enabled) => (entry, enabled),
            (other, _) => mismatch("enum entry", &other),
        })
        .collect();
    let is_open = results.next().into_bool();
    if !is_extern {
        context.report_error("non-extern enums are not supported yet");
    }
    if !is_valid_type_name(&name.name) {
        naming_convention_error(context, "Type", &name.name, "UpperCamelCase");
    }
    if constexpr_generates.as_deref() == Some(name.name.as_str()) {
        context.lint(format!(
            "Unnecessary 'constexpr' clause for enum {}",
            name.name
        ));
    }
    let generate_nonconstexpr = base_type_expression.is_some();
    if let Some((base, _)) = &base_type_expression {
        add_constexpr(context, base);
    }
    for (entry, enabled) in &entries {
        if *enabled
            && let Some((_, pos)) = &entry.ty
            && !generate_nonconstexpr
        {
            context.error_at(
                "Enum constants with custom types require an enum with an extends clause.",
                *pos,
            );
        }
    }
    Some(Value::Decls(vec![Decl::Enum {
        name,
        extends: base_type_expression.map(|(ty, _)| ty),
        entries: entries
            .into_iter()
            .map(|(entry, _)| (entry.name, entry.ty.map(|(ty, _)| ty)))
            .collect(),
        is_open,
        annotations: Vec::new(),
    }]))
}

fn make_namespace_declaration(
    context: &mut ActionContext,
    results: &mut ParseResultIterator,
) -> Option<Value> {
    let (name, span) = results.next().into_str();
    if !is_snake_case(&name) {
        naming_convention_error(context, "Namespace", &name, "snake_case");
    }
    let body = results.next().into_decls();
    Some(Value::Decls(vec![Decl::Namespace {
        name: Ident { name, span },
        body,
    }]))
}

fn concat_list(_context: &mut ActionContext, results: &mut ParseResultIterator) -> Option<Value> {
    let declarations = results
        .next()
        .into_list()
        .into_iter()
        .flat_map(Value::into_decls)
        .collect();
    Some(Value::Decls(declarations))
}

fn process_torque_import_declaration(
    context: &mut ActionContext,
    results: &mut ParseResultIterator,
) -> Option<Value> {
    let (path, span) = results.next().into_str();
    if !context.aborted {
        context.imports.push(ImportCheck {
            path: path.clone(),
            span: results.span(),
            message_index: context.messages.len(),
        });
    }
    context.declarations.push(Decl::Include { path, span });
    None
}

fn add_global_declarations(
    context: &mut ActionContext,
    results: &mut ParseResultIterator,
) -> Option<Value> {
    let declarations = results.next().into_decls();
    context.declarations.extend(declarations);
    None
}

#[derive(Default)]
struct GrammarBuilder {
    symbol_rules: Vec<Vec<RuleId>>,
    rules: Vec<Rule>,
    keywords: BTreeMap<Vec<u8>, SymbolId>,
    patterns: BTreeMap<Pattern, SymbolId>,
}

impl GrammarBuilder {
    fn new_symbol(&mut self) -> SymbolId {
        self.symbol_rules.push(Vec::new());
        (self.symbol_rules.len() - 1) as SymbolId
    }

    fn add_rule(&mut self, left: SymbolId, right: Vec<SymbolId>, action: Action) {
        let id = self.rules.len() as RuleId;
        self.rules.push(Rule {
            left,
            right,
            action,
        });
        self.symbol_rules[left as usize].push(id);
    }

    fn new_symbol_with(&mut self, rules: Vec<(Vec<SymbolId>, Action)>) -> SymbolId {
        let symbol = self.new_symbol();
        for (right, action) in rules {
            self.add_rule(symbol, right, action);
        }
        symbol
    }

    fn token(&mut self, keyword: &str) -> SymbolId {
        if let Some(symbol) = self.keywords.get(keyword.as_bytes()) {
            return *symbol;
        }
        let symbol = self.new_symbol();
        self.keywords.insert(keyword.as_bytes().to_vec(), symbol);
        symbol
    }

    fn pattern(&mut self, pattern: Pattern) -> SymbolId {
        if let Some(symbol) = self.patterns.get(&pattern) {
            return *symbol;
        }
        let symbol = self.new_symbol();
        self.patterns.insert(pattern, symbol);
        symbol
    }

    fn sequence(&mut self, symbols: Vec<SymbolId>) -> SymbolId {
        self.new_symbol_with(vec![(symbols, default_action)])
    }

    fn try_or_default(&mut self, symbol: SymbolId, cast: Action, default: Action) -> SymbolId {
        self.new_symbol_with(vec![(vec![symbol], cast), (Vec::new(), default)])
    }

    fn nonempty_list(&mut self, element: SymbolId, separator: Option<SymbolId>) -> SymbolId {
        let list = self.new_symbol();
        self.add_rule(list, vec![element], make_singleton_vector);
        match separator {
            Some(separator) => {
                self.add_rule(list, vec![list, separator, element], make_extended_vector)
            }
            None => self.add_rule(list, vec![list, element], make_extended_vector),
        }
        list
    }

    fn list(&mut self, element: SymbolId, separator: Option<SymbolId>) -> SymbolId {
        let list = self.nonempty_list(element, separator);
        self.try_or_default(list, default_action, yield_empty_list)
    }

    fn optional(&mut self, symbol: SymbolId) -> SymbolId {
        self.try_or_default(symbol, cast_to_optional, yield_none)
    }

    fn try_or_empty_list(&mut self, symbol: SymbolId) -> SymbolId {
        self.try_or_default(symbol, default_action, yield_empty_list)
    }

    fn check_if(&mut self, symbol: SymbolId) -> SymbolId {
        self.new_symbol_with(vec![(vec![symbol], yield_true), (Vec::new(), yield_false)])
    }

    fn one_of(&mut self, alternatives: &[&str]) -> SymbolId {
        let result = self.new_symbol();
        for alternative in alternatives {
            let token = self.token(alternative);
            self.add_rule(result, vec![token], make_identifier_from_matched_input);
        }
        result
    }

    fn binary_operator(&mut self, next_level: SymbolId, op: SymbolId) -> SymbolId {
        let result = self.new_symbol();
        self.add_rule(result, vec![next_level], default_action);
        self.add_rule(result, vec![result, op, next_level], make_binary_operator);
        result
    }

    fn nonempty_list_allow_if_annotation(
        &mut self,
        element: SymbolId,
        separator: Option<SymbolId>,
        annotations: SymbolId,
    ) -> SymbolId {
        let list = self.new_symbol();
        self.add_rule(
            list,
            vec![annotations, element],
            make_extended_vector_if_annotation_first,
        );
        match separator {
            Some(separator) => self.add_rule(
                list,
                vec![list, annotations, separator, element],
                make_extended_vector_if_annotation,
            ),
            None => self.add_rule(
                list,
                vec![list, annotations, element],
                make_extended_vector_if_annotation,
            ),
        }
        list
    }

    fn list_allow_if_annotation(
        &mut self,
        element: SymbolId,
        separator: Option<SymbolId>,
        annotations: SymbolId,
    ) -> SymbolId {
        let list = self.nonempty_list_allow_if_annotation(element, separator, annotations);
        self.try_or_default(list, default_action, yield_empty_list)
    }
}

fn build_grammar() -> Grammar {
    let mut b = GrammarBuilder::default();

    let identifier = b.new_symbol();
    let name = b.new_symbol();
    let annotation_name = b.new_symbol();
    let intrinsic_name = b.new_symbol();
    let string_literal = b.new_symbol();
    let external_string = b.new_symbol();
    let integer_literal = b.new_symbol();
    let floating_point_literal = b.new_symbol();
    let int32_literal = b.new_symbol();
    let annotation_parameter = b.new_symbol();
    let annotation_parameters = b.new_symbol();
    let annotation = b.new_symbol();
    let namespace_qualification = b.new_symbol();
    let simple_type = b.new_symbol();
    let type_ = b.new_symbol();
    let generic_parameter = b.new_symbol();
    let generic_parameters = b.new_symbol();
    let generic_specialization_type_list = b.new_symbol();
    let implicit_parameter_list = b.new_symbol();
    let type_list_maybe_var_args = b.new_symbol();
    let label_parameter = b.new_symbol();
    let return_type = b.new_symbol();
    let name_and_type = b.new_symbol();
    let class_field = b.new_symbol();
    let struct_field = b.new_symbol();
    let bit_field_declaration = b.new_symbol();
    let parameter_list_no_vararg = b.new_symbol();
    let parameter_list_allow_vararg = b.new_symbol();
    let increment_decrement_operator = b.new_symbol();
    let identifier_expression = b.new_symbol();
    let argument_list = b.new_symbol();
    let call_expression = b.new_symbol();
    let call_method_expression = b.new_symbol();
    let named_expression = b.new_symbol();
    let initializer_list = b.new_symbol();
    let intrinsic_call_expression = b.new_symbol();
    let new_expression = b.new_symbol();
    let primary_expression = b.new_symbol();
    let unary_expression = b.new_symbol();
    let shift_operator = b.new_symbol();
    let relational_expression = b.new_symbol();
    let logical_and_expression = b.new_symbol();
    let logical_or_expression = b.new_symbol();
    let conditional_expression = b.new_symbol();
    let assignment_operator = b.new_symbol();
    let assignment_expression = b.new_symbol();
    let block = b.new_symbol();
    let try_handler = b.new_symbol();
    let expression_with_source = b.new_symbol();
    let enum_entry = b.new_symbol();
    let var_declaration = b.new_symbol();
    let var_declaration_with_initialization = b.new_symbol();
    let atomar_statement = b.new_symbol();
    let statement = b.new_symbol();
    let typeswitch_case = b.new_symbol();
    let optional_body = b.new_symbol();
    let method = b.new_symbol();
    let optional_class_body = b.new_symbol();
    let declaration = b.new_symbol();
    let declaration_list = b.new_symbol();
    let file = b.new_symbol();
    let expression = assignment_expression;

    let identifier_pattern = b.pattern(Pattern::Identifier);
    let t_runtime = b.token("runtime");
    b.add_rule(identifier, vec![identifier_pattern], yield_matched_input);
    b.add_rule(identifier, vec![t_runtime], yield_matched_input);

    b.add_rule(name, vec![identifier], make_identifier);

    let annotation_pattern = b.pattern(Pattern::Annotation);
    b.add_rule(
        annotation_name,
        vec![annotation_pattern],
        make_identifier_from_matched_input,
    );

    let intrinsic_pattern = b.pattern(Pattern::IntrinsicName);
    b.add_rule(
        intrinsic_name,
        vec![intrinsic_pattern],
        make_identifier_from_matched_input,
    );

    let string_pattern = b.pattern(Pattern::StringLiteral);
    b.add_rule(string_literal, vec![string_pattern], yield_matched_input);

    b.add_rule(
        external_string,
        vec![string_literal],
        string_literal_unquote_action,
    );

    let integer_pattern = b.pattern(Pattern::IntegerLiteral);
    let hex_pattern = b.pattern(Pattern::HexLiteral);
    b.add_rule(
        integer_literal,
        vec![integer_pattern],
        yield_integer_literal,
    );
    b.add_rule(integer_literal, vec![hex_pattern], yield_integer_literal);

    let float_pattern = b.pattern(Pattern::FloatingPointLiteral);
    b.add_rule(floating_point_literal, vec![float_pattern], yield_double);

    b.add_rule(int32_literal, vec![integer_pattern], yield_int32);
    b.add_rule(int32_literal, vec![hex_pattern], yield_int32);

    b.add_rule(
        annotation_parameter,
        vec![identifier],
        make_string_annotation_parameter,
    );
    b.add_rule(
        annotation_parameter,
        vec![int32_literal],
        make_int_annotation_parameter,
    );
    b.add_rule(
        annotation_parameter,
        vec![external_string],
        make_string_annotation_parameter,
    );

    let t_lparen = b.token("(");
    let t_rparen = b.token(")");
    b.add_rule(
        annotation_parameters,
        vec![t_lparen, annotation_parameter, t_rparen],
        default_action,
    );

    let optional_annotation_parameters = b.optional(annotation_parameters);
    b.add_rule(
        annotation,
        vec![annotation_name, optional_annotation_parameters],
        make_annotation,
    );

    let annotations = b.list(annotation, None);

    let t_double_colon = b.token("::");
    let global_namespace = b.check_if(t_double_colon);
    let qualifier = b.sequence(vec![identifier, t_double_colon]);
    let qualifiers = b.list(qualifier, None);
    b.add_rule(
        namespace_qualification,
        vec![global_namespace, qualifiers],
        make_namespace_qualification,
    );

    let t_comma = b.token(",");
    let type_list = b.list(type_, Some(t_comma));

    b.add_rule(simple_type, vec![t_lparen, type_, t_rparen], default_action);
    let t_constexpr = b.token("constexpr");
    let is_constexpr = b.check_if(t_constexpr);
    let basic_generic_arguments = b.try_or_empty_list(generic_specialization_type_list);
    b.add_rule(
        simple_type,
        vec![
            namespace_qualification,
            is_constexpr,
            identifier,
            basic_generic_arguments,
        ],
        make_basic_type_expression,
    );
    let t_builtin = b.token("builtin");
    let t_fat_arrow = b.token("=>");
    b.add_rule(
        simple_type,
        vec![
            t_builtin,
            t_lparen,
            type_list,
            t_rparen,
            t_fat_arrow,
            simple_type,
        ],
        make_function_type_expression,
    );
    let t_const = b.token("const");
    let reference_is_const = b.check_if(t_const);
    let t_ampersand = b.token("&");
    b.add_rule(
        simple_type,
        vec![reference_is_const, t_ampersand, simple_type],
        make_reference_type_expression,
    );

    let t_bar = b.token("|");
    b.add_rule(type_, vec![simple_type], default_action);
    b.add_rule(
        type_,
        vec![type_, t_bar, simple_type],
        make_union_type_expression,
    );

    let t_colon = b.token(":");
    let t_type = b.token("type");
    let t_extends = b.token("extends");
    let generic_constraint = b.sequence(vec![t_extends, type_]);
    let optional_generic_constraint = b.optional(generic_constraint);
    b.add_rule(
        generic_parameter,
        vec![name, t_colon, t_type, optional_generic_constraint],
        make_generic_parameter,
    );

    let t_less = b.token("<");
    let t_greater = b.token(">");
    let generic_parameter_list = b.list(generic_parameter, Some(t_comma));
    b.add_rule(
        generic_parameters,
        vec![t_less, generic_parameter_list, t_greater],
        default_action,
    );

    b.add_rule(
        generic_specialization_type_list,
        vec![t_less, type_list, t_greater],
        default_action,
    );

    let _optional_generic_parameters = b.optional(generic_parameters);

    let implicit_kind = b.one_of(&["implicit", "js-implicit"]);
    let implicit_parameters = b.list(name_and_type, Some(t_comma));
    b.add_rule(
        implicit_parameter_list,
        vec![t_lparen, implicit_kind, implicit_parameters, t_rparen],
        make_implicit_parameter_list,
    );

    let optional_implicit_parameter_list = b.optional(implicit_parameter_list);

    let type_followed_by_comma = b.sequence(vec![type_, t_comma]);
    let types_followed_by_comma = b.list(type_followed_by_comma, None);
    let t_ellipsis = b.token("...");
    b.add_rule(
        type_list_maybe_var_args,
        vec![
            optional_implicit_parameter_list,
            t_lparen,
            types_followed_by_comma,
            t_ellipsis,
            t_rparen,
        ],
        make_parameter_list_varargs_types,
    );
    b.add_rule(
        type_list_maybe_var_args,
        vec![
            optional_implicit_parameter_list,
            t_lparen,
            type_list,
            t_rparen,
        ],
        make_parameter_list_types,
    );

    let label_types = b.sequence(vec![t_lparen, type_list, t_rparen]);
    let optional_label_types = b.try_or_empty_list(label_types);
    b.add_rule(
        label_parameter,
        vec![name, optional_label_types],
        make_label_and_types,
    );

    b.add_rule(return_type, vec![t_colon, type_], default_action);
    b.add_rule(return_type, Vec::new(), deprecated_make_void_type);

    let t_labels = b.token("labels");
    let label_parameters = b.nonempty_list(label_parameter, Some(t_comma));
    let label_list = b.sequence(vec![t_labels, label_parameters]);
    let optional_label_list = b.try_or_empty_list(label_list);

    let t_otherwise = b.token("otherwise");
    let otherwise_statements = b.nonempty_list(atomar_statement, Some(t_comma));
    let otherwise = b.sequence(vec![t_otherwise, otherwise_statements]);
    let optional_otherwise = b.try_or_empty_list(otherwise);

    b.add_rule(
        name_and_type,
        vec![name, t_colon, type_],
        make_name_and_type,
    );

    let t_lbracket = b.token("[");
    let t_rbracket = b.token("]");
    let array_specifier = b.sequence(vec![t_lbracket, expression, t_rbracket]);
    let optional_array_specifier = b.optional(array_specifier);

    let t_weak = b.token("weak");
    let field_is_weak = b.check_if(t_weak);
    let class_field_is_const = b.check_if(t_const);
    let t_question = b.token("?");
    let field_is_optional = b.check_if(t_question);
    let t_semicolon = b.token(";");
    b.add_rule(
        class_field,
        vec![
            annotations,
            field_is_weak,
            class_field_is_const,
            name,
            field_is_optional,
            optional_array_specifier,
            t_colon,
            type_,
            t_semicolon,
        ],
        make_class_field,
    );

    let struct_field_is_const = b.check_if(t_const);
    b.add_rule(
        struct_field,
        vec![struct_field_is_const, name, t_colon, type_, t_semicolon],
        make_struct_field,
    );

    let t_bit = b.token("bit");
    b.add_rule(
        bit_field_declaration,
        vec![
            name,
            t_colon,
            type_,
            t_colon,
            int32_literal,
            t_bit,
            t_semicolon,
        ],
        make_bit_field_declaration,
    );

    let named_parameters = b.list(name_and_type, Some(t_comma));
    b.add_rule(
        parameter_list_no_vararg,
        vec![
            optional_implicit_parameter_list,
            t_lparen,
            named_parameters,
            t_rparen,
        ],
        make_parameter_list_named,
    );

    b.add_rule(
        parameter_list_allow_vararg,
        vec![parameter_list_no_vararg],
        default_action,
    );
    let named_parameter_followed_by_comma = b.sequence(vec![name_and_type, t_comma]);
    let named_parameters_followed_by_comma = b.list(named_parameter_followed_by_comma, None);
    b.add_rule(
        parameter_list_allow_vararg,
        vec![
            optional_implicit_parameter_list,
            t_lparen,
            named_parameters_followed_by_comma,
            t_ellipsis,
            identifier,
            t_rparen,
        ],
        make_parameter_list_named_varargs,
    );

    let t_increment = b.token("++");
    let t_decrement = b.token("--");
    b.add_rule(
        increment_decrement_operator,
        vec![t_increment],
        yield_increment,
    );
    b.add_rule(
        increment_decrement_operator,
        vec![t_decrement],
        yield_decrement,
    );

    let identifier_generic_arguments = b.try_or_empty_list(generic_specialization_type_list);
    b.add_rule(
        identifier_expression,
        vec![namespace_qualification, name, identifier_generic_arguments],
        make_identifier_expression,
    );

    let arguments = b.list(expression, Some(t_comma));
    b.add_rule(
        argument_list,
        vec![t_lparen, arguments, t_rparen],
        default_action,
    );

    b.add_rule(
        call_expression,
        vec![identifier_expression, argument_list, optional_otherwise],
        make_call,
    );

    let t_dot = b.token(".");
    b.add_rule(
        call_method_expression,
        vec![
            primary_expression,
            t_dot,
            name,
            argument_list,
            optional_otherwise,
        ],
        make_method_call,
    );

    b.add_rule(
        named_expression,
        vec![name, t_colon, expression],
        make_name_and_expression,
    );
    b.add_rule(
        named_expression,
        vec![expression],
        make_name_and_expression_from_expression,
    );

    let t_lbrace = b.token("{");
    let t_rbrace = b.token("}");
    let named_expressions = b.list(named_expression, Some(t_comma));
    b.add_rule(
        initializer_list,
        vec![t_lbrace, named_expressions, t_rbrace],
        default_action,
    );

    let intrinsic_generic_arguments = b.try_or_empty_list(generic_specialization_type_list);
    b.add_rule(
        intrinsic_call_expression,
        vec![intrinsic_name, intrinsic_generic_arguments, argument_list],
        make_intrinsic_call_expression,
    );

    let t_new = b.token("new");
    let t_pretenured = b.token("Pretenured");
    let pretenured_marker = b.sequence(vec![t_lparen, t_pretenured, t_rparen]);
    let is_pretenured = b.check_if(pretenured_marker);
    let t_clear_padding = b.token("ClearPadding");
    let clear_padding_marker = b.sequence(vec![t_lparen, t_clear_padding, t_rparen]);
    let clear_padding = b.check_if(clear_padding_marker);
    b.add_rule(
        new_expression,
        vec![
            t_new,
            is_pretenured,
            clear_padding,
            simple_type,
            initializer_list,
        ],
        make_new_expression,
    );

    b.add_rule(primary_expression, vec![call_expression], default_action);
    b.add_rule(
        primary_expression,
        vec![call_method_expression],
        default_action,
    );
    b.add_rule(
        primary_expression,
        vec![intrinsic_call_expression],
        default_action,
    );
    b.add_rule(
        primary_expression,
        vec![identifier_expression],
        default_action,
    );
    b.add_rule(
        primary_expression,
        vec![primary_expression, t_dot, name],
        make_field_access_expression,
    );
    let t_arrow = b.token("->");
    b.add_rule(
        primary_expression,
        vec![primary_expression, t_arrow, name],
        make_reference_field_access_expression,
    );
    b.add_rule(
        primary_expression,
        vec![primary_expression, t_lbracket, expression, t_rbracket],
        make_element_access_expression,
    );
    b.add_rule(
        primary_expression,
        vec![integer_literal],
        make_integer_literal_expression,
    );
    b.add_rule(
        primary_expression,
        vec![floating_point_literal],
        make_floating_point_literal_expression,
    );
    b.add_rule(
        primary_expression,
        vec![string_literal],
        make_string_literal_expression,
    );
    b.add_rule(
        primary_expression,
        vec![simple_type, initializer_list],
        make_struct_expression,
    );
    b.add_rule(primary_expression, vec![new_expression], default_action);
    b.add_rule(
        primary_expression,
        vec![t_lparen, expression, t_rparen],
        default_action,
    );

    b.add_rule(unary_expression, vec![primary_expression], default_action);
    let unary_operators = b.one_of(&["+", "-", "!", "~", "&"]);
    b.add_rule(
        unary_expression,
        vec![unary_operators, unary_expression],
        make_unary_operator,
    );
    let t_star = b.token("*");
    b.add_rule(
        unary_expression,
        vec![t_star, unary_expression],
        make_dereference_expression,
    );
    b.add_rule(
        unary_expression,
        vec![t_ellipsis, unary_expression],
        make_spread_expression,
    );
    b.add_rule(
        unary_expression,
        vec![increment_decrement_operator, unary_expression],
        make_increment_decrement_expression_prefix,
    );
    b.add_rule(
        unary_expression,
        vec![unary_expression, increment_decrement_operator],
        make_increment_decrement_expression_postfix,
    );

    let multiplicative_operators = b.one_of(&["*", "/", "%"]);
    let multiplicative_expression = b.binary_operator(unary_expression, multiplicative_operators);

    let additive_operators = b.one_of(&["+", "-"]);
    let additive_expression = b.binary_operator(multiplicative_expression, additive_operators);

    let t_shift_left = b.token("<<");
    b.add_rule(
        shift_operator,
        vec![t_shift_left],
        make_identifier_from_matched_input,
    );
    b.add_rule(
        shift_operator,
        vec![t_greater, t_greater],
        make_right_shift_identifier,
    );
    b.add_rule(
        shift_operator,
        vec![t_greater, t_greater, t_greater],
        make_right_shift_identifier,
    );

    let shift_expression = b.binary_operator(additive_expression, shift_operator);

    b.add_rule(
        relational_expression,
        vec![shift_expression],
        default_action,
    );
    let relational_operators = b.one_of(&["<", ">", "<=", ">="]);
    b.add_rule(
        relational_expression,
        vec![shift_expression, relational_operators, shift_expression],
        make_binary_operator,
    );

    let equality_operators = b.one_of(&["==", "!="]);
    let equality_expression = b.binary_operator(relational_expression, equality_operators);

    let bitwise_operators = b.one_of(&["&", "|"]);
    let bitwise_expression = b.binary_operator(equality_expression, bitwise_operators);

    let t_logical_and = b.token("&&");
    b.add_rule(
        logical_and_expression,
        vec![bitwise_expression],
        default_action,
    );
    b.add_rule(
        logical_and_expression,
        vec![logical_and_expression, t_logical_and, bitwise_expression],
        make_logical_and_expression,
    );

    let t_logical_or = b.token("||");
    b.add_rule(
        logical_or_expression,
        vec![logical_and_expression],
        default_action,
    );
    b.add_rule(
        logical_or_expression,
        vec![logical_or_expression, t_logical_or, logical_and_expression],
        make_logical_or_expression,
    );

    b.add_rule(
        conditional_expression,
        vec![logical_or_expression],
        default_action,
    );
    b.add_rule(
        conditional_expression,
        vec![
            logical_or_expression,
            t_question,
            expression,
            t_colon,
            conditional_expression,
        ],
        make_conditional_expression,
    );

    let t_assign = b.token("=");
    b.add_rule(assignment_operator, vec![t_assign], yield_none);
    let compound_assignment_operators = b.one_of(&[
        "*=", "/=", "%=", "+=", "-=", "<<=", ">>=", ">>>=", "&=", "^=", "|=",
    ]);
    b.add_rule(
        assignment_operator,
        vec![compound_assignment_operators],
        extract_assignment_operator,
    );

    b.add_rule(
        assignment_expression,
        vec![conditional_expression],
        default_action,
    );
    b.add_rule(
        assignment_expression,
        vec![
            conditional_expression,
            assignment_operator,
            assignment_expression,
        ],
        make_assignment_expression,
    );

    let t_deferred = b.token("deferred");
    let is_deferred = b.check_if(t_deferred);
    let block_statements = b.list_allow_if_annotation(statement, None, annotations);
    b.add_rule(
        block,
        vec![is_deferred, t_lbrace, block_statements, t_rbrace],
        make_block_statement,
    );

    let t_label = b.token("label");
    let label_block_parameters = b.try_or_default(
        parameter_list_no_vararg,
        default_action,
        yield_empty_parameter_list,
    );
    b.add_rule(
        try_handler,
        vec![t_label, name, label_block_parameters, block],
        make_label_block,
    );
    let t_catch = b.token("catch");
    let catch_parameters = b.list(identifier, Some(t_comma));
    b.add_rule(
        try_handler,
        vec![t_catch, t_lparen, catch_parameters, t_rparen, block],
        make_catch_block,
    );

    b.add_rule(
        expression_with_source,
        vec![expression],
        make_expression_with_source,
    );

    let type_specifier = b.sequence(vec![t_colon, type_]);
    let optional_type_specifier = b.optional(type_specifier);

    b.add_rule(
        enum_entry,
        vec![annotations, name, optional_type_specifier],
        make_enum_entry,
    );

    let var_kind = b.one_of(&["let", "const"]);
    b.add_rule(
        var_declaration,
        vec![var_kind, name, optional_type_specifier],
        make_var_declaration_statement,
    );

    let initialized_var_kind = b.one_of(&["let", "const"]);
    b.add_rule(
        var_declaration_with_initialization,
        vec![
            initialized_var_kind,
            name,
            optional_type_specifier,
            t_assign,
            expression,
        ],
        make_var_declaration_statement,
    );

    b.add_rule(
        atomar_statement,
        vec![expression],
        make_expression_statement,
    );
    let t_return = b.token("return");
    let optional_return_value = b.optional(expression);
    b.add_rule(
        atomar_statement,
        vec![t_return, optional_return_value],
        make_return_statement,
    );
    let t_tail = b.token("tail");
    b.add_rule(
        atomar_statement,
        vec![t_tail, call_expression],
        make_tail_call_statement,
    );
    let t_break = b.token("break");
    b.add_rule(atomar_statement, vec![t_break], make_break_statement);
    let t_continue = b.token("continue");
    b.add_rule(atomar_statement, vec![t_continue], make_continue_statement);
    let t_goto = b.token("goto");
    let goto_arguments = b.try_or_empty_list(argument_list);
    b.add_rule(
        atomar_statement,
        vec![t_goto, name, goto_arguments],
        make_goto_statement,
    );
    let debug_kind = b.one_of(&["debug", "unreachable"]);
    b.add_rule(atomar_statement, vec![debug_kind], make_debug_statement);

    b.add_rule(statement, vec![block], default_action);
    b.add_rule(
        statement,
        vec![atomar_statement, t_semicolon],
        default_action,
    );
    b.add_rule(
        statement,
        vec![var_declaration, t_semicolon],
        default_action,
    );
    b.add_rule(
        statement,
        vec![var_declaration_with_initialization, t_semicolon],
        default_action,
    );
    let t_if = b.token("if");
    let if_is_constexpr = b.check_if(t_constexpr);
    let t_else = b.token("else");
    let else_branch = b.sequence(vec![t_else, statement]);
    let optional_else_branch = b.optional(else_branch);
    b.add_rule(
        statement,
        vec![
            t_if,
            if_is_constexpr,
            t_lparen,
            expression,
            t_rparen,
            statement,
            optional_else_branch,
        ],
        make_if_statement,
    );
    let t_typeswitch = b.token("typeswitch");
    let typeswitch_cases = b.nonempty_list_allow_if_annotation(typeswitch_case, None, annotations);
    b.add_rule(
        statement,
        vec![
            t_typeswitch,
            t_lparen,
            expression,
            t_rparen,
            t_lbrace,
            typeswitch_cases,
            t_rbrace,
        ],
        make_typeswitch_statement,
    );
    let t_try = b.token("try");
    let try_handlers = b.list(try_handler, None);
    b.add_rule(
        statement,
        vec![t_try, block, try_handlers],
        make_try_label_expression,
    );
    let assert_kind = b.one_of(&["dcheck", "check", "sbxcheck", "static_assert"]);
    b.add_rule(
        statement,
        vec![
            assert_kind,
            t_lparen,
            expression_with_source,
            t_rparen,
            t_semicolon,
        ],
        make_assert_statement,
    );
    let t_while = b.token("while");
    b.add_rule(
        statement,
        vec![t_while, t_lparen, expression, t_rparen, statement],
        make_while_statement,
    );
    let t_for = b.token("for");
    let for_initializer = b.optional(var_declaration_with_initialization);
    let for_test = b.optional(expression);
    let for_action = b.optional(expression);
    b.add_rule(
        statement,
        vec![
            t_for,
            t_lparen,
            for_initializer,
            t_semicolon,
            for_test,
            t_semicolon,
            for_action,
            t_rparen,
            statement,
        ],
        make_for_loop_statement,
    );

    let t_case = b.token("case");
    let case_name = b.sequence(vec![name, t_colon]);
    let optional_case_name = b.optional(case_name);
    b.add_rule(
        typeswitch_case,
        vec![
            t_case,
            t_lparen,
            optional_case_name,
            type_,
            t_rparen,
            t_colon,
            block,
        ],
        make_typeswitch_case,
    );

    b.add_rule(optional_body, vec![block], cast_to_optional);
    b.add_rule(optional_body, vec![t_semicolon], yield_none);

    let t_transitioning = b.token("transitioning");
    let method_is_transitioning = b.check_if(t_transitioning);
    let t_operator = b.token("operator");
    let method_operator = b.sequence(vec![t_operator, external_string]);
    let optional_method_operator = b.optional(method_operator);
    let t_macro = b.token("macro");
    b.add_rule(
        method,
        vec![
            method_is_transitioning,
            optional_method_operator,
            t_macro,
            name,
            parameter_list_no_vararg,
            return_type,
            optional_label_list,
            block,
        ],
        make_method_declaration,
    );

    let class_methods = b.list(method, None);
    let class_fields = b.list(class_field, None);
    b.add_rule(
        optional_class_body,
        vec![t_lbrace, class_methods, class_fields, t_rbrace],
        make_class_body,
    );
    b.add_rule(optional_class_body, vec![t_semicolon], yield_none);

    b.add_rule(
        declaration,
        vec![
            annotations,
            t_const,
            name,
            t_colon,
            type_,
            t_assign,
            expression,
            t_semicolon,
        ],
        make_const_declaration,
    );
    let t_generates = b.token("generates");
    b.add_rule(
        declaration,
        vec![
            t_const,
            name,
            t_colon,
            type_,
            t_generates,
            external_string,
            t_semicolon,
        ],
        make_extern_const_declaration,
    );
    let t_extern = b.token("extern");
    let class_is_extern = b.check_if(t_extern);
    let t_transient = b.token("transient");
    let class_is_transient = b.check_if(t_transient);
    let class_kind = b.one_of(&["class", "shape"]);
    let class_generates = b.sequence(vec![t_generates, external_string]);
    let optional_class_generates = b.optional(class_generates);
    b.add_rule(
        declaration,
        vec![
            annotations,
            class_is_extern,
            class_is_transient,
            class_kind,
            name,
            t_extends,
            type_,
            optional_class_generates,
            optional_class_body,
        ],
        make_class_declaration,
    );
    let t_struct = b.token("struct");
    let struct_generic_parameters = b.try_or_empty_list(generic_parameters);
    let struct_methods = b.list_allow_if_annotation(method, None, annotations);
    let struct_fields = b.list_allow_if_annotation(struct_field, None, annotations);
    b.add_rule(
        declaration,
        vec![
            annotations,
            t_struct,
            name,
            struct_generic_parameters,
            t_lbrace,
            struct_methods,
            struct_fields,
            t_rbrace,
        ],
        make_struct_declaration,
    );
    let t_bitfield = b.token("bitfield");
    let bit_fields = b.list_allow_if_annotation(bit_field_declaration, None, annotations);
    b.add_rule(
        declaration,
        vec![
            annotations,
            t_bitfield,
            t_struct,
            name,
            t_extends,
            type_,
            t_lbrace,
            bit_fields,
            t_rbrace,
        ],
        make_bit_field_struct_declaration,
    );
    let abstract_type_is_transient = b.check_if(t_transient);
    let abstract_type_generic_parameters = b.try_or_empty_list(generic_parameters);
    let abstract_type_extends = b.sequence(vec![t_extends, type_]);
    let optional_abstract_type_extends = b.optional(abstract_type_extends);
    let abstract_type_generates = b.sequence(vec![t_generates, external_string]);
    let optional_abstract_type_generates = b.optional(abstract_type_generates);
    let abstract_type_constexpr = b.sequence(vec![t_constexpr, external_string]);
    let optional_abstract_type_constexpr = b.optional(abstract_type_constexpr);
    b.add_rule(
        declaration,
        vec![
            annotations,
            abstract_type_is_transient,
            t_type,
            name,
            abstract_type_generic_parameters,
            optional_abstract_type_extends,
            optional_abstract_type_generates,
            optional_abstract_type_constexpr,
            t_semicolon,
        ],
        make_abstract_type_declaration,
    );
    b.add_rule(
        declaration,
        vec![annotations, t_type, name, t_assign, type_, t_semicolon],
        make_type_alias_declaration,
    );
    let t_intrinsic = b.token("intrinsic");
    let intrinsic_generic_parameters = b.try_or_empty_list(generic_parameters);
    b.add_rule(
        declaration,
        vec![
            t_intrinsic,
            intrinsic_name,
            intrinsic_generic_parameters,
            parameter_list_no_vararg,
            return_type,
            optional_body,
        ],
        make_intrinsic_declaration,
    );
    let extern_macro_is_transitioning = b.check_if(t_transitioning);
    let extern_macro_operator = b.sequence(vec![t_operator, external_string]);
    let optional_extern_macro_operator = b.optional(extern_macro_operator);
    let extern_macro_assembler = b.sequence(vec![identifier, t_double_colon]);
    let optional_extern_macro_assembler = b.optional(extern_macro_assembler);
    let extern_macro_generic_parameters = b.try_or_empty_list(generic_parameters);
    b.add_rule(
        declaration,
        vec![
            annotations,
            t_extern,
            extern_macro_is_transitioning,
            optional_extern_macro_operator,
            t_macro,
            optional_extern_macro_assembler,
            name,
            extern_macro_generic_parameters,
            type_list_maybe_var_args,
            return_type,
            optional_label_list,
            t_semicolon,
        ],
        make_external_macro,
    );
    let extern_builtin_is_transitioning = b.check_if(t_transitioning);
    let t_javascript = b.token("javascript");
    let extern_builtin_is_javascript = b.check_if(t_javascript);
    let extern_builtin_generic_parameters = b.try_or_empty_list(generic_parameters);
    b.add_rule(
        declaration,
        vec![
            annotations,
            t_extern,
            extern_builtin_is_transitioning,
            extern_builtin_is_javascript,
            t_builtin,
            name,
            extern_builtin_generic_parameters,
            type_list_maybe_var_args,
            return_type,
            t_semicolon,
        ],
        make_external_builtin,
    );
    let extern_runtime_is_transitioning = b.check_if(t_transitioning);
    b.add_rule(
        declaration,
        vec![
            t_extern,
            extern_runtime_is_transitioning,
            t_runtime,
            name,
            type_list_maybe_var_args,
            return_type,
            t_semicolon,
        ],
        make_external_runtime,
    );
    let macro_is_transitioning = b.check_if(t_transitioning);
    let macro_operator = b.sequence(vec![t_operator, external_string]);
    let optional_macro_operator = b.optional(macro_operator);
    let macro_generic_parameters = b.try_or_empty_list(generic_parameters);
    b.add_rule(
        declaration,
        vec![
            annotations,
            macro_is_transitioning,
            optional_macro_operator,
            t_macro,
            name,
            macro_generic_parameters,
            parameter_list_no_vararg,
            return_type,
            optional_label_list,
            optional_body,
        ],
        make_torque_macro_declaration,
    );
    let builtin_is_transitioning = b.check_if(t_transitioning);
    let builtin_is_javascript = b.check_if(t_javascript);
    let builtin_generic_parameters = b.try_or_empty_list(generic_parameters);
    b.add_rule(
        declaration,
        vec![
            annotations,
            builtin_is_transitioning,
            builtin_is_javascript,
            t_builtin,
            name,
            builtin_generic_parameters,
            parameter_list_allow_vararg,
            return_type,
            optional_body,
        ],
        make_torque_builtin_declaration,
    );
    let specialization_is_transitioning = b.check_if(t_transitioning);
    b.add_rule(
        declaration,
        vec![
            annotations,
            specialization_is_transitioning,
            name,
            generic_specialization_type_list,
            parameter_list_allow_vararg,
            return_type,
            optional_label_list,
            block,
        ],
        make_specialization_declaration,
    );
    let t_include = b.token("#include");
    let include_selector = b.sequence(vec![t_lbracket, external_string, t_rbracket]);
    let optional_include_selector = b.optional(include_selector);
    b.add_rule(
        declaration,
        vec![t_include, optional_include_selector, external_string],
        make_cpp_include_declaration,
    );
    let enum_is_extern = b.check_if(t_extern);
    let t_enum = b.token("enum");
    let enum_extends = b.sequence(vec![t_extends, type_]);
    let optional_enum_extends = b.optional(enum_extends);
    let enum_constexpr = b.sequence(vec![t_constexpr, external_string]);
    let optional_enum_constexpr = b.optional(enum_constexpr);
    let enum_entries = b.nonempty_list_allow_if_annotation(enum_entry, Some(t_comma), annotations);
    let open_enum_marker = b.sequence(vec![t_comma, t_ellipsis]);
    let enum_is_open = b.check_if(open_enum_marker);
    b.add_rule(
        declaration,
        vec![
            enum_is_extern,
            t_enum,
            name,
            optional_enum_extends,
            optional_enum_constexpr,
            t_lbrace,
            enum_entries,
            enum_is_open,
            t_rbrace,
        ],
        make_enum_declaration,
    );
    let t_namespace = b.token("namespace");
    b.add_rule(
        declaration,
        vec![
            t_namespace,
            identifier,
            t_lbrace,
            declaration_list,
            t_rbrace,
        ],
        make_namespace_declaration,
    );

    let declarations = b.list(declaration, None);
    b.add_rule(declaration_list, vec![declarations], concat_list);

    let t_import = b.token("import");
    b.add_rule(
        file,
        vec![file, t_import, external_string],
        process_torque_import_declaration,
    );
    b.add_rule(file, vec![file, declaration], add_global_declarations);
    b.add_rule(file, Vec::new(), default_action);

    let top_level = b.new_symbol();
    b.add_rule(top_level, vec![file], default_action);
    let top_level_rule = (b.rules.len() - 1) as RuleId;

    let rule_nullable = nullable_rules(&b.symbol_rules, &b.rules);
    let (dotted_base, dotted_count) = dotted_rule_bases(&b.rules);
    let keywords_by_first_byte = keywords_by_first_byte(&b.keywords);
    Grammar {
        symbol_rules: b.symbol_rules,
        rules: b.rules,
        keywords_by_first_byte,
        patterns: b.patterns,
        top_level_rule,
        rule_nullable,
        dotted_base,
        dotted_count,
    }
}

fn grammar() -> &'static Grammar {
    static GRAMMAR: OnceLock<Grammar> = OnceLock::new();
    GRAMMAR.get_or_init(build_grammar)
}

pub(crate) struct ParseResult {
    pub(crate) declarations: Option<Vec<Decl>>,
    pub(crate) messages: Vec<Message>,
    pub(crate) imports: Vec<ImportCheck>,
}

fn failure_result(file: u32, failure: Failure) -> ParseResult {
    ParseResult {
        declarations: None,
        messages: vec![Message {
            kind: MessageKind::Error,
            message: failure.message,
            span: Span::new(file, failure.begin, failure.end),
        }],
        imports: Vec::new(),
    }
}

pub(crate) fn parse_torque(text: &str, file: u32) -> ParseResult {
    let grammar = grammar();
    let bytes = text.as_bytes();
    let tokens = match run_lexer(grammar, bytes) {
        Ok(tokens) => tokens,
        Err(failure) => return failure_result(file, failure),
    };
    let (chart, root) = match run_earley(grammar, &tokens, bytes) {
        Ok(parsed) => parsed,
        Err(failure) => return failure_result(file, failure),
    };
    let mut context = ActionContext {
        file,
        text: bytes,
        tokens: &tokens,
        current: Span::new(file, 0, 0),
        messages: Vec::new(),
        aborted: false,
        declarations: Vec::new(),
        imports: Vec::new(),
    };
    run_actions(&chart, root, &mut context);
    ParseResult {
        declarations: Some(context.declarations),
        messages: context.messages,
        imports: context.imports,
    }
}
