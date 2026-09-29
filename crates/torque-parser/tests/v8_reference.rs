use torque_parser::parse_file;

fn messages(source: &str) -> Vec<(&'static str, String, u32, u32)> {
    parse_file("memory://reference.tq".into(), source.into(), 0)
        .diagnostics
        .into_iter()
        .map(|item| (item.severity, item.message, item.start, item.end))
        .collect()
}

fn error(message: &str, start: u32, end: u32) -> (&'static str, String, u32, u32) {
    ("error", message.into(), start, end)
}

fn lint(message: &str, start: u32, end: u32) -> (&'static str, String, u32, u32) {
    ("warning", message.into(), start, end)
}

#[test]
fn lexer_errors_match_v8() {
    assert_eq!(
        messages("macro F(): void { let x = a # b; }"),
        [error("Lexer Error: unknown token \"# b; }\"", 28, 28)]
    );
    assert_eq!(
        messages("macro F(): void { a = b ^ c; }"),
        [error("Lexer Error: unknown token \"^ c; }\"", 24, 24)]
    );
    assert_eq!(
        messages("macro F(): void { let __x = 1; }"),
        [error("Lexer Error: unknown token \"__x = 1; }\"", 22, 22)]
    );
}

#[test]
fn parser_errors_match_v8() {
    assert_eq!(
        messages("macro F(): void { a = b-1; }"),
        [error("Parser Error: unexpected token \"-1\"", 23, 25)]
    );
    assert_eq!(
        messages("macro F(): void { let type = 1; }"),
        [error("Parser Error: unexpected token \"type\"", 22, 26)]
    );
    assert_eq!(
        messages("extern macro F(x: Smi): Smi;"),
        [error("Parser Error: unexpected token \":\"", 16, 17)]
    );
    assert_eq!(
        messages("extern enum E extends int32 { kA, kB, }"),
        [error("Parser Error: unexpected token \"}\"", 38, 39)]
    );
    assert_eq!(
        messages("macro F(): void { if (true) {"),
        [error("Parser Error: unexpected end of input", 29, 29)]
    );
}

#[test]
fn ambiguous_input_is_rejected_like_v8() {
    assert_eq!(
        messages("macro F(): void { a = -x++; }"),
        [error(
            "Ambiguous grammer rules for \"-x++\":\n   -x  ++\nvs\n   -  x++",
            26,
            27
        )]
    );
}

#[test]
fn action_errors_match_v8() {
    assert_eq!(
        messages("macro F(): void { a = b > > c; }"),
        [error(
            "right-shift operators may not contain any whitespace",
            24,
            27
        )]
    );
    assert_eq!(
        messages("macro F(): void { if (a) b(); else c(); }"),
        [error("if-else statements require curly braces", 18, 39)]
    );
    assert_eq!(
        messages("macro F(): void { let x; }"),
        [error("Declaration is missing a type.", 18, 23)]
    );
    assert_eq!(
        messages("macro F() {}"),
        [error(
            "Default void return types are deprecated. Add `: void`.",
            10,
            11
        )]
    );
    assert_eq!(
        messages("@if(NOT_A_FLAG) const kX: int32 = 1;"),
        [error(
            "Unknown flag used in @if: NOT_A_FLAG. Please add it to the list in BuildFlags.",
            0,
            36
        )]
    );
    assert_eq!(
        messages("builtin F(js-implicit context: Context)(): void {}"),
        [error(
            "\"js-implicit\" is for implicit parameters passed according to the JavaScript \
             calling convention. Use \"implicit\" instead.",
            0,
            50
        )]
    );
}

#[test]
fn lints_match_v8() {
    assert_eq!(
        messages("macro lowerName(x_y: Smi): void {}"),
        [
            lint(
                "Parameter \"x_y\" does not follow \"lowerCamelCase\" naming convention.",
                16,
                19
            ),
            lint(
                "Macro \"lowerName\" does not follow \"UpperCamelCase\" naming convention.",
                6,
                15
            ),
        ]
    );
    assert_eq!(
        messages("@abstract macro F(): void {}"),
        [lint("Annotation @abstract is not allowed here", 0, 9)]
    );
}

#[test]
fn failed_parses_still_produce_declarations_for_editor_features() {
    let output = parse_file(
        "memory://recovery.tq".into(),
        "macro Complete(): void {}\nmacro Broken(): void {".into(),
        0,
    );
    assert_eq!(
        output.diagnostics[0].message,
        "Parser Error: unexpected end of input"
    );
    assert!(!output.file.decls.is_empty());
}
