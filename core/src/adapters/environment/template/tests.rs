use std::path::Path;

use super::*;

#[test]
fn parses_literals_bindings_defaults_and_comments() {
    let parsed = TemplateParser.parse(
        Path::new(".env.example"),
        "# comment\nDB_HOST= # mm:from=DB.HOST required\nDB_PORT=5432 # mm:from=DB.PORT\nHASH='a # b' # ordinary\nexport EMPTY=\n",
    );

    assert!(parsed.diagnostics.is_empty());
    assert_eq!(parsed.variables.len(), 4);
    assert_eq!(parsed.variables[0].global_key.as_deref(), Some("DB.HOST"));
    assert!(parsed.variables[0].required);
    assert_eq!(parsed.variables[1].default_value.as_deref(), Some("5432"));
    assert_eq!(parsed.variables[2].literal_value.as_deref(), Some("a # b"));
    assert_eq!(parsed.variables[3].literal_value.as_deref(), Some(""));
}

#[test]
fn reports_duplicates_and_unknown_directives() {
    let parsed = TemplateParser.parse(Path::new(".env.example"), "A=1\nA=2\nB= # mm:wat=NOPE\n");
    assert_eq!(parsed.diagnostics.len(), 2);
    assert_eq!(parsed.diagnostics[0].code, "MM_ENV_VARIABLE_DUPLICATE");
    assert_eq!(parsed.diagnostics[1].code, "MM_ENV_TEMPLATE_DIRECTIVE");
}

#[test]
fn rejects_malformed_quotes_that_would_hide_a_directive() {
    let parsed = TemplateParser.parse(
        Path::new(".env.example"),
        "TOKEN=oops\" # mm:from=AUTH.TOKEN required\nOTHER=\"x\"junk\"\n",
    );
    assert_eq!(parsed.diagnostics.len(), 2);
    assert!(parsed.variables.is_empty());
    assert!(
        parsed
            .diagnostics
            .iter()
            .all(|diagnostic| diagnostic.code == "MM_ENV_TEMPLATE_VALUE")
    );
}

#[test]
fn parser_rejects_nuls_assignments_names_directives_controls_and_every_quote_error() {
    let nul = TemplateParser.parse(Path::new("template"), "A=1\0B=2");
    assert_eq!(nul.diagnostics[0].code, "MM_ENV_TEMPLATE_NUL");
    assert!(nul.variables.is_empty());

    let cases = [
        ("NO_ASSIGNMENT", "MM_ENV_TEMPLATE_ASSIGNMENT"),
        ("1INVALID=value", "MM_ENV_VARIABLE_INVALID"),
        ("VALUE=a\u{1}", "MM_ENV_TEMPLATE_VALUE"),
        ("VALUE= # mm:required", "MM_ENV_TEMPLATE_DIRECTIVE"),
        ("VALUE= # mm:from=invalid-key", "MM_ENV_GLOBAL_INVALID"),
        ("VALUE= # mm:from=A.B from=C.D", "MM_ENV_TEMPLATE_DIRECTIVE"),
        (
            "VALUE= # mm:from=A.B required required",
            "MM_ENV_TEMPLATE_DIRECTIVE",
        ),
        ("VALUE= # mm:", "MM_ENV_TEMPLATE_DIRECTIVE"),
        ("VALUE='unterminated", "MM_ENV_TEMPLATE_VALUE"),
        ("VALUE='a'b'", "MM_ENV_TEMPLATE_VALUE"),
        ("VALUE=\"unterminated", "MM_ENV_TEMPLATE_VALUE"),
        ("VALUE=\"a\"junk", "MM_ENV_TEMPLATE_VALUE"),
        ("VALUE=a'b", "MM_ENV_TEMPLATE_VALUE"),
        ("VALUE=\"ends\\\"", "MM_ENV_TEMPLATE_VALUE"),
        ("VALUE=\"bad\\x\"", "MM_ENV_TEMPLATE_VALUE"),
        ("VALUE=\"line\\nfeed\"", "MM_ENV_TEMPLATE_VALUE"),
    ];
    for (source, code) in cases {
        let parsed = TemplateParser.parse(Path::new("template"), source);
        assert_eq!(parsed.diagnostics.len(), 1, "{source}");
        assert_eq!(parsed.diagnostics[0].code, code, "{source}");
    }

    let quoted = TemplateParser.parse(
        Path::new("template"),
        "VALUE=\"slash\\\\ quote\\\"\" # comment\r\n",
    );
    assert!(quoted.diagnostics.is_empty());
    assert_eq!(
        quoted.variables[0].literal_value.as_deref(),
        Some("slash\\ quote\"")
    );
}
