use neomax_core::issues::Issue;

pub(super) fn list_line(issue: &Issue) -> String {
    let mirrors = issue
        .repos
        .iter()
        .map(|(name, mirror)| {
            mirror.number.as_deref().map_or_else(
                || format!("{name}:local"),
                |number| format!("{name}#{number}"),
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    format!(
        "{:<24} [{:<8}] {}{}",
        issue.key,
        issue.status.as_str(),
        issue.title.chars().take(64).collect::<String>(),
        if mirrors.is_empty() {
            String::new()
        } else {
            format!(" ({mirrors})")
        }
    )
}

pub(super) fn print_detail(issue: &Issue) {
    println!("{} [{}] {}", issue.key, issue.status.as_str(), issue.title);
    println!("project: {}", issue.project);
    if !issue.body.is_empty() {
        println!("body: {}", issue.body);
    }
    for (name, mirror) in &issue.repos {
        println!(
            "repo {name}: {}",
            mirror.url.as_deref().unwrap_or("(local, no mirror)")
        );
    }
    if !issue.runs.is_empty() {
        println!("runs: {}", issue.runs.join(", "));
    }
    if !issue.pull_requests.is_empty() {
        let links = issue
            .pull_requests
            .iter()
            .map(|(repo, url)| format!("{repo}={url}"))
            .collect::<Vec<_>>()
            .join(", ");
        println!("PRs: {links}");
    }
}
