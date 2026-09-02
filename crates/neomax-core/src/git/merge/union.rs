use std::collections::HashSet;

pub fn union_resolve(text: &str) -> Option<String> {
    if text.lines().any(|line| line.starts_with("|||||||")) {
        return None;
    }
    let lines = text.split('\n').collect::<Vec<_>>();
    let mut output = Vec::new();
    let mut index = 0;
    while index < lines.len() {
        if !lines[index].starts_with("<<<<<<<") {
            output.push(lines[index]);
            index += 1;
            continue;
        }
        index += 1;
        let mut ours = Vec::new();
        while index < lines.len() && !lines[index].starts_with("=======") {
            if lines[index].starts_with("|||||||") {
                return None;
            }
            ours.push(lines[index]);
            index += 1;
        }
        if index == lines.len() {
            return None;
        }
        index += 1;
        let mut theirs = Vec::new();
        while index < lines.len() && !lines[index].starts_with(">>>>>>>") {
            theirs.push(lines[index]);
            index += 1;
        }
        if index == lines.len() {
            return None;
        }
        index += 1;
        let mut seen = ours.iter().copied().collect::<HashSet<_>>();
        output.extend(ours.iter().copied());
        for line in theirs {
            if seen.insert(line) {
                output.push(line);
            }
        }
    }
    let resolved = output.join("\n");
    (!resolved.contains("<<<<<<<") && !resolved.contains(">>>>>>>")).then_some(resolved)
}
