use super::inspector::WindowsProcessInfo;

pub(super) fn image_basename(image_path: &str) -> &str {
    image_path.rsplit(['\\', '/']).next().unwrap_or(image_path)
}

pub(super) fn recognized_claude_image(image: &str) -> bool {
    matches!(
        image_basename(image).to_ascii_lowercase().as_str(),
        "claude"
            | "claude.exe"
            | "claude.cmd"
            | "claude.ps1"
            | "node"
            | "node.exe"
            | "bun"
            | "bun.exe"
            | "deno"
            | "deno.exe"
            | "cmd.exe"
            | "powershell.exe"
            | "pwsh.exe"
    )
}

pub(crate) fn is_claude_process(process: &WindowsProcessInfo) -> bool {
    let image = image_basename(&process.image_path).to_ascii_lowercase();
    if matches!(
        image.as_str(),
        "claude" | "claude.exe" | "claude.cmd" | "claude.ps1"
    ) {
        return true;
    }
    if !matches!(
        image.as_str(),
        "node"
            | "node.exe"
            | "bun"
            | "bun.exe"
            | "deno"
            | "deno.exe"
            | "cmd.exe"
            | "powershell.exe"
            | "pwsh.exe"
    ) {
        return false;
    }
    process
        .command_line
        .as_deref()
        .is_some_and(command_line_has_claude_shim)
}

fn command_line_has_claude_shim(command_line: &str) -> bool {
    let arguments = windows_arguments(command_line);
    arguments
        .iter()
        .enumerate()
        .take(8)
        .any(|(index, argument)| {
            let lower = argument.to_ascii_lowercase();
            let base = image_basename(&lower);
            if matches!(base, "claude.cmd" | "claude.ps1" | "claude.exe") {
                return true;
            }
            if base == "claude" {
                return index > 0
                    && !matches!(
                        arguments.get(index.wrapping_sub(1)).map(String::as_str),
                        Some("-p") | Some("--prompt") | Some("--message")
                    );
            }
            base == "cli.js" && lower.contains("anthropic-ai") && lower.contains("claude-code")
        })
}

pub(crate) fn profile_environment_value(command_line: &str, key: &str) -> Option<String> {
    let command_lower = command_line.to_ascii_lowercase();
    let marker = format!("{}=", key.to_ascii_lowercase());
    let mut found = None;
    for (offset, _) in command_lower.match_indices(marker.as_str()) {
        if offset != 0 && !command_line.as_bytes()[offset - 1].is_ascii_whitespace() {
            continue;
        }
        let value = &command_line[offset + marker.len()..];
        let Some(value) = (|| {
            if let Some(quoted) = value.strip_prefix('"') {
                let end = quoted.find('"')?;
                Some(&quoted[..end])
            } else {
                let end = next_environment_boundary(value).unwrap_or(value.len());
                Some(value[..end].trim())
            }
        })() else {
            continue;
        };
        if !value.is_empty() && !value.chars().any(char::is_control) {
            found = Some(value.to_owned());
        }
    }
    found
}

pub(super) fn environment_end(bytes: &[u8]) -> Option<usize> {
    bytes
        .len()
        .checked_sub(4)
        .and_then(|last| {
            (0..=last)
                .step_by(2)
                .find(|&index| bytes[index..index + 4] == [0, 0, 0, 0])
        })
        .map(|index| index + 4)
}

fn next_environment_boundary(value: &str) -> Option<usize> {
    let bytes = value.as_bytes();
    for index in 1..bytes.len() {
        if !bytes[index].is_ascii_whitespace() {
            continue;
        }
        let mut start = index;
        while start < bytes.len() && bytes[start].is_ascii_whitespace() {
            start += 1;
        }
        let mut end = start;
        while end < bytes.len()
            && (bytes[end].is_ascii_uppercase()
                || bytes[end].is_ascii_digit()
                || bytes[end] == b'_')
        {
            end += 1;
        }
        if end > start && bytes.get(end) == Some(&b'=') {
            return Some(index);
        }
    }
    None
}

fn windows_arguments(command_line: &str) -> Vec<String> {
    let mut arguments = Vec::new();
    let mut argument = String::new();
    let mut quoted = false;
    let mut slashes = 0usize;
    for character in command_line.chars() {
        match character {
            '\\' => slashes += 1,
            '"' => {
                argument.extend(std::iter::repeat_n('\\', slashes / 2));
                if slashes % 2 == 1 {
                    argument.push('"');
                } else {
                    quoted = !quoted;
                }
                slashes = 0;
            }
            character if character.is_ascii_whitespace() && !quoted => {
                argument.extend(std::iter::repeat_n('\\', slashes));
                slashes = 0;
                if !argument.is_empty() {
                    arguments.push(std::mem::take(&mut argument));
                }
            }
            character => {
                argument.extend(std::iter::repeat_n('\\', slashes));
                slashes = 0;
                argument.push(character);
            }
        }
    }
    argument.extend(std::iter::repeat_n('\\', slashes));
    if !argument.is_empty() {
        arguments.push(argument);
    }
    arguments
}

#[cfg(test)]
#[path = "parsing_tests.rs"]
mod tests;
