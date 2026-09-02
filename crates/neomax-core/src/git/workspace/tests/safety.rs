use super::super::{generated_part_branch, validate_plan_id, validate_ref_name};

#[test]
fn rejects_unsafe_ids_and_branch_names_before_git_changes() {
    assert!(validate_plan_id("../escape").is_err());
    assert!(generated_part_branch("plan", "part;touch").is_err());
    assert!(validate_ref_name("neomax/../main").is_err());
    assert!(validate_ref_name("-bad").is_err());
    assert!(validate_ref_name("neomax/part.lock").is_err());
    assert!(validate_ref_name("@").is_err());
}
