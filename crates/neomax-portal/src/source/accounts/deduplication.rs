use std::collections::BTreeMap;

use crate::model::AccountView;

pub(crate) fn mark_duplicate_accounts(accounts: &mut [AccountView]) {
    let mut by_email = BTreeMap::<String, Vec<usize>>::new();
    for (index, account) in accounts.iter().enumerate() {
        if account.authenticated {
            if let Some(email) = account.email.as_deref().filter(|value| !value.is_empty()) {
                by_email
                    .entry(email.to_ascii_lowercase())
                    .or_default()
                    .push(index);
            }
        }
    }
    for account in accounts.iter_mut() {
        account.duplicate_of.clear();
    }
    for indexes in by_email.into_values().filter(|indexes| indexes.len() > 1) {
        let members = indexes
            .iter()
            .map(|index| accounts[*index].n.clone())
            .collect::<Vec<_>>();
        for index in indexes {
            accounts[index].duplicate_of = members.clone();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn account(n: &str, email: &str, authenticated: bool) -> AccountView {
        AccountView {
            n: n.into(),
            email: Some(email.into()),
            authenticated,
            ..AccountView::default()
        }
    }

    #[test]
    fn marks_only_authenticated_accounts_with_matching_identity() {
        let mut accounts = vec![
            account("1", "Dev@Example.test", true),
            account("2", "dev@example.test", true),
            account("3", "dev@example.test", false),
        ];
        mark_duplicate_accounts(&mut accounts);
        assert_eq!(accounts[0].duplicate_of, vec!["1", "2"]);
        assert_eq!(accounts[1].duplicate_of, vec!["1", "2"]);
        assert!(accounts[2].duplicate_of.is_empty());
    }
}
