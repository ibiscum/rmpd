use rmpd_core::error::RmpdError;
use rmpd_core::filter::FilterExpression;

fn parse_error(input: &str) -> String {
    match FilterExpression::parse(input, false) {
        Err(RmpdError::ParseError(msg)) => msg,
        Err(other) => panic!("expected parse error, got: {other:?}"),
        Ok(expr) => panic!("expected parse failure, got: {expr:?}"),
    }
}

#[test]
fn invalid_empty_expression_rejected() {
    let msg = parse_error("   ");
    assert!(msg.contains("'(' expected"), "unexpected: {msg}");
}

#[test]
fn invalid_unterminated_string_rejected() {
    let msg = parse_error("(artist == 'Radiohead)");
    assert!(msg.contains("Closing quote not found"), "unexpected: {msg}");
}

#[test]
fn invalid_unquoted_value_rejected() {
    let msg = parse_error("(artist == Radiohead)");
    assert!(msg.contains("Quoted string expected"), "unexpected: {msg}");
}

#[test]
fn invalid_missing_operator_rejected() {
    let msg = parse_error("(artist 'Radiohead')");
    assert!(msg.contains("Unknown filter operator"), "unexpected: {msg}");
}

#[test]
fn invalid_dangling_boolean_operator_rejected() {
    let msg = parse_error("((artist == 'Radiohead') AND)");
    assert!(msg.contains("'(' expected"), "unexpected: {msg}");
}

#[test]
fn invalid_unbalanced_parenthesis_rejected() {
    let msg = parse_error("((artist == 'Radiohead')");
    assert!(msg.contains("Word expected"), "unexpected: {msg}");
}

#[test]
fn invalid_trailing_tokens_rejected() {
    let msg = parse_error("(artist == 'Radiohead') garbage");
    assert!(msg.contains("Unparsed garbage after expression"), "unexpected: {msg}");
}