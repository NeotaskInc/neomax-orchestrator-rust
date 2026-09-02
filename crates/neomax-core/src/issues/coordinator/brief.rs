use super::super::types::Issue;
use super::driver::MirrorDriver;
use super::service::CrossRepoIssueCoordinator;
use super::types::RepositoryCatalog;

impl<'a, C, D> CrossRepoIssueCoordinator<'a, C, D>
where
    C: RepositoryCatalog,
    D: MirrorDriver,
{
    pub fn issue_brief(&self, issue: &Issue) -> String {
        let repos = if issue.repos.is_empty() {
            "(all project repos)".into()
        } else {
            issue
                .repos
                .iter()
                .map(|(name, mirror)| {
                    if mirror.url.is_some() {
                        name.clone()
                    } else {
                        format!("{name} (local)")
                    }
                })
                .collect::<Vec<_>>()
                .join(", ")
        };
        let mirrors = issue
            .repos
            .iter()
            .map(|(name, mirror)| {
                format!("  - {name}: {}", mirror.url.as_deref().unwrap_or("(local)"))
            })
            .collect::<Vec<_>>()
            .join("\n");
        format!(
            "OBJECTIVE - FIX cross-repo issue `{}` (project `{}`): {}\n\nCONTEXT/WHY:\n{}\n\nAFFECTED REPOS:\n{}\n\nMIRROR ISSUES:\n{}",
            issue.key,
            issue.project,
            issue.title,
            if issue.body.is_empty() {
                "(none)"
            } else {
                &issue.body
            },
            repos,
            if mirrors.is_empty() {
                "(none)"
            } else {
                &mirrors
            },
        )
    }
}
