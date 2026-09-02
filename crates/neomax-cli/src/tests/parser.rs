use std::ffi::OsString;

use crate::parser;

#[test]
fn converts_utf8_arguments_without_reordering() {
    let args = parser::utf8_args(vec![OsString::from("--project"), OsString::from("demo")])
        .expect("valid UTF-8");
    assert_eq!(args, ["--project", "demo"]);
}

#[cfg(unix)]
#[test]
fn rejects_non_utf8_arguments() {
    use std::os::unix::ffi::OsStringExt;

    let error = parser::utf8_args(vec![OsString::from_vec(vec![0xff])])
        .expect_err("invalid UTF-8 should fail");
    assert!(error.to_string().contains("UTF-8"));
}
