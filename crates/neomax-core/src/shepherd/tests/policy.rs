use crate::shepherd::{MergePolicy, billing_ignore_enabled};

#[test]
fn billing_environment_defaults_to_ignore_and_only_zero_disables_it() {
    assert!(billing_ignore_enabled(None));
    assert!(billing_ignore_enabled(Some("1")));
    assert!(!billing_ignore_enabled(Some("0")));
    assert!(billing_ignore_enabled(Some("false")));
    assert_eq!(
        MergePolicy::from_billing_environment(Some("0")),
        MergePolicy {
            ignore_non_running_ci: false
        }
    );
}
