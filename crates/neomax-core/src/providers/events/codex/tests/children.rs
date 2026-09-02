use serde_json::json;

use super::super::super::common::stream;
use super::super::parse;

#[test]
fn distinguishes_steps_from_native_agents() {
    let output = parse(&stream(&[
        json!({"type":"thread.started","thread_id":"t1"}),
        json!({"type":"item.started","item":{"id":"c1","type":"command_execution","command":"cargo test"}}),
        json!({"type":"item.started","item":{"id":"a1","type":"collab_agent","name":"helper"}}),
        json!({"type":"turn.completed","usage":{"input_tokens":10,"output_tokens":2}}),
    ]));
    assert_eq!(output.subtype.as_deref(), Some("success"));
    assert_eq!(output.children[0].kind, "step");
    assert_eq!(output.children[1].kind, "agent");
    assert!(
        output
            .children
            .iter()
            .all(|child| child.status == "completed")
    );
}
