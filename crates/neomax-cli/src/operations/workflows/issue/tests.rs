use super::parser;

#[test]
fn requires_an_issue_subcommand() {
    assert!(parser::parse(&[]).is_err());
}

#[test]
fn parses_issue_subcommand_and_arguments() {
    let args = vec![
        "list".to_owned(),
        "--project=demo".to_owned(),
        "--json".to_owned(),
    ];
    let (subcommand, parsed) = parser::parse(&args).expect("issue arguments should parse");
    assert_eq!(subcommand, "list");
    assert_eq!(parsed.value("--project"), Some("demo"));
    assert!(parsed.has("--json"));
}
