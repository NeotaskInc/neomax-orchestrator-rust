pub fn project_slug(value: &str) -> String {
    let slug = value
        .chars()
        .filter(|character| character.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect::<String>();
    if slug.is_empty() {
        "project".into()
    } else {
        slug
    }
}
