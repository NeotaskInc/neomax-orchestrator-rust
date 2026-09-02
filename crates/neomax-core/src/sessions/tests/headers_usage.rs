use crate::sessions::headers::claude_token_usage;

#[test]
fn token_usage_sums_all_assistant_turns() {
    let text = r#"{"type":"assistant","message":{"usage":{"input_tokens":5,"output_tokens":7,"cache_read_input_tokens":2}}}
{"type":"assistant","message":{"usage":{"input_tokens":3,"output_tokens":4,"cache_creation_input_tokens":1}}}"#;
    let tokens = claude_token_usage(text);
    assert_eq!(tokens.input, 8);
    assert_eq!(tokens.output, 11);
    assert_eq!(tokens.cache_read, 2);
    assert_eq!(tokens.cache_write, 1);
}
