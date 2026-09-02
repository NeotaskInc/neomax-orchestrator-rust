//! Provider executable discovery and Windows command-shell safety.

use std::ffi::{OsStr, OsString};
use std::fs;
use std::path::{Path, PathBuf};

use crate::runtime::platform::RuntimePlatform;
use crate::Result;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedProviderExecutable {
    pub program: OsString,
    pub prefix_args: Vec<OsString>,
    pub resolved_path: Option<PathBuf>,
    pub uses_command_shell: bool,
}

impl ResolvedProviderExecutable {
    pub fn process_command(&self, args: &[OsString]) -> crate::Result<std::process::Command> {
        let mut command = std::process::Command::new(&self.program);
        self.apply_to_process(&mut command, args)?;
        Ok(command)
    }

    #[cfg(test)]
    pub(crate) fn apply_to_command(
        &self,
        args: &[OsString],
    ) -> crate::Result<(OsString, Vec<OsString>)> {
        if !self.uses_command_shell {
            let mut combined = self.prefix_args.clone();
            combined.extend_from_slice(args);
            return Ok((self.program.clone(), combined));
        }
        let command = self.command_line(args)?;
        let mut output = self.prefix_args.clone();
        output.push(OsString::from(command));
        Ok((self.program.clone(), output))
    }

    fn apply_to_process(
        &self,
        command: &mut std::process::Command,
        args: &[OsString],
    ) -> crate::Result<()> {
        if !self.uses_command_shell {
            command.args(&self.prefix_args).args(args);
            return Ok(());
        }

        command.args(&self.prefix_args);
        let command_line = self.command_line(args)?;
        let wrapped = format!(r#""{command_line}""#);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.raw_arg(wrapped);
        }
        #[cfg(not(windows))]
        {
            command.arg(wrapped);
        }
        Ok(())
    }

    fn command_line(&self, args: &[OsString]) -> crate::Result<String> {
        let path = self
            .resolved_path
            .as_deref()
            .unwrap_or_else(|| Path::new(self.program.as_os_str()));
        let mut command = quote_cmd_argument(path.as_os_str())?;
        for arg in args {
            command.push(' ');
            command.push_str(&quote_cmd_argument(arg)?);
        }
        Ok(command)
    }
}

pub fn resolve_provider_executable(
    program: &OsStr,
    platform: RuntimePlatform,
    path_value: Option<&OsStr>,
    pathext_value: Option<&OsStr>,
    comspec: Option<&OsStr>,
    system_root: Option<&OsStr>,
    current_dir: &Path,
) -> Result<ResolvedProviderExecutable> {
    if !platform.is_windows() {
        return Ok(ResolvedProviderExecutable {
            program: program.to_os_string(),
            prefix_args: Vec::new(),
            resolved_path: None,
            uses_command_shell: false,
        });
    }
    let input = Path::new(program);
    if crate::io::is_rooted_but_not_absolute(input) {
        return Err(crate::Error::InvalidArgument(format!(
            "Windows provider executable path is rooted but not absolute: {}",
            input.display()
        )));
    }
    let current_dir_text = current_dir.to_string_lossy();
    if !is_windows_absolute(current_dir, &current_dir_text) {
        return Err(crate::Error::InvalidArgument(format!(
            "Windows provider current directory must be absolute: {}",
            current_dir.display()
        )));
    }
    let resolved = resolve_windows_path(program, path_value, pathext_value, current_dir);
    let path = resolved.clone().unwrap_or_else(|| PathBuf::from(program));
    let is_script = path
        .extension()
        .and_then(OsStr::to_str)
        .is_some_and(|extension| matches!(extension.to_ascii_lowercase().as_str(), "cmd" | "bat"));
    if !is_script {
        return Ok(ResolvedProviderExecutable {
            program: resolved
                .map(|path| path.into_os_string())
                .unwrap_or_else(|| program.to_os_string()),
            prefix_args: Vec::new(),
            resolved_path: None,
            uses_command_shell: false,
        });
    }
    let shell = resolve_command_shell(comspec, system_root)?;
    Ok(ResolvedProviderExecutable {
        program: shell,
        prefix_args: vec![
            OsString::from("/d"),
            OsString::from("/e:on"),
            OsString::from("/v:off"),
            OsString::from("/c"),
        ],
        resolved_path: Some(path),
        uses_command_shell: true,
    })
}

fn resolve_windows_path(
    program: &OsStr,
    path_value: Option<&OsStr>,
    pathext_value: Option<&OsStr>,
    current_dir: &Path,
) -> Option<PathBuf> {
    let input = PathBuf::from(program);
    if crate::io::is_rooted_but_not_absolute(&input) {
        return None;
    }
    if input.components().count() > 1 || input.is_absolute() {
        let candidate = if input.is_absolute() {
            input
        } else {
            current_dir.join(input)
        };
        return is_file_candidate(&candidate).then_some(candidate);
    }
    let mut extensions = vec![String::new()];
    if input.extension().is_none() {
        let raw = pathext_value
            .and_then(OsStr::to_str)
            .unwrap_or(".COM;.EXE;.BAT;.CMD");
        extensions.extend(
            raw.split(';')
                .filter(|extension| !extension.is_empty())
                .map(|extension| extension.to_ascii_lowercase()),
        );
    }
    let paths = path_value.and_then(OsStr::to_str).unwrap_or_default();
    let mut roots = Vec::with_capacity(paths.len().saturating_add(1));
    roots.push(current_dir.to_string_lossy().into_owned());
    roots.extend(
        paths
            .split(';')
            .filter(|item| !item.is_empty())
            .map(ToOwned::to_owned),
    );
    for root in roots {
        let root_path = Path::new(&root);
        if !is_windows_absolute(root_path, &root) {
            continue;
        }
        for extension in &extensions {
            let candidate =
                PathBuf::from(&root).join(format!("{}{}", input.to_string_lossy(), extension));
            if is_file_candidate(&candidate) {
                return Some(candidate);
            }
        }
    }
    None
}

fn is_file_candidate(path: &Path) -> bool {
    path.is_file()
}

pub(crate) fn resolve_command_shell(
    comspec: Option<&OsStr>,
    system_root: Option<&OsStr>,
) -> Result<OsString> {
    if let Some(path) = comspec.and_then(validated_command_shell) {
        return Ok(path);
    }
    let fallback = system_root
        .and_then(safe_windows_root)
        .map(|root| root.join("System32").join("cmd.exe"))
        .and_then(|path| validated_command_shell(path.as_os_str()));
    fallback.ok_or_else(|| {
        crate::Error::InvalidArgument(
            "Windows command shell is unavailable; ComSpec and SystemRoot are invalid".into(),
        )
    })
}

fn validated_command_shell(value: &OsStr) -> Option<OsString> {
    let text = value.to_str()?;
    if text.is_empty() || text.chars().any(char::is_control) {
        return None;
    }
    let path = Path::new(value);
    if !is_windows_absolute(path, text) {
        return None;
    }
    let file_name = path.file_name().and_then(OsStr::to_str)?;
    if !file_name.eq_ignore_ascii_case("cmd.exe") {
        return None;
    }
    let metadata = fs::symlink_metadata(path).ok()?;
    let linked = has_linked_component(path).ok()?;
    if !metadata.file_type().is_file()
        || metadata.file_type().is_symlink()
        || is_reparse_point(&metadata)
        || linked
    {
        return None;
    }
    Some(path.to_owned().into_os_string())
}

fn safe_windows_root(value: &OsStr) -> Option<PathBuf> {
    let text = value.to_str()?;
    if text.is_empty() || text.chars().any(char::is_control) {
        return None;
    }
    let path = Path::new(value);
    if !is_windows_absolute(path, text) {
        return None;
    }
    let metadata = fs::symlink_metadata(path).ok()?;
    let linked = has_linked_component(path).ok()?;
    if !metadata.file_type().is_dir()
        || metadata.file_type().is_symlink()
        || is_reparse_point(&metadata)
        || linked
    {
        return None;
    }
    Some(path.to_owned())
}

fn is_reparse_point(metadata: &fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        metadata.file_attributes() & 0x400 != 0
    }
    #[cfg(not(windows))]
    {
        let _ = metadata;
        false
    }
}

pub(super) fn has_linked_component(path: &Path) -> std::io::Result<bool> {
    for ancestor in path.ancestors() {
        if ancestor.as_os_str().is_empty() {
            continue;
        }
        let metadata = fs::symlink_metadata(ancestor)?;
        if metadata.file_type().is_symlink() || is_reparse_point(&metadata) {
            return Ok(true);
        }
    }
    Ok(false)
}

fn is_windows_absolute(path: &Path, text: &str) -> bool {
    if crate::io::is_rooted_but_not_absolute(path) {
        return false;
    }
    path.is_absolute()
        || text.starts_with('\\')
        || text.starts_with('/')
        || (text.as_bytes().get(1) == Some(&b':')
            && text
                .as_bytes()
                .get(2)
                .is_some_and(|separator| *separator == b'/' || *separator == b'\\'))
}

pub(crate) fn quote_cmd_argument(value: &OsStr) -> crate::Result<String> {
    let normalized = normalize_cmd_argument(value)?;
    let text = normalized.as_str();
    quote_cmd_text(text)
}

fn normalize_cmd_argument(value: &OsStr) -> crate::Result<String> {
    let text = value.to_str().ok_or_else(|| {
        crate::Error::InvalidArgument("Windows cmd provider arguments must be valid Unicode".into())
    })?;
    if text.contains('\0') {
        return Err(crate::Error::InvalidArgument(
            "Windows cmd provider arguments must not contain NUL".into(),
        ));
    }
    if text.contains('\r') || text.contains('\n') {
        // cmd.exe cannot safely carry CR or LF in an argument. Unicode line
        // separators preserve multiline prompt semantics without reparsing.
        Ok(text
            .replace("\r\n", "\u{2028}")
            .replace(['\r', '\n'], "\u{2028}"))
    } else {
        Ok(text.to_owned())
    }
}

fn quote_cmd_text(text: &str) -> crate::Result<String> {
    let needs_quotes = text.is_empty()
        || text.ends_with('\\')
        || text.chars().any(|character| {
            character.is_control()
                || (character.is_ascii()
                    && !(character.is_ascii_alphanumeric() || r"#$*+-./:?@\_".contains(character)))
        });
    let mut quoted = String::with_capacity(text.len() + usize::from(needs_quotes) * 2);
    if needs_quotes {
        quoted.push('"');
    }
    let mut backslashes = 0usize;
    for character in text.chars() {
        if character == '\\' {
            backslashes += 1;
            continue;
        }
        if character == '"' {
            quoted.extend(std::iter::repeat_n('\\', backslashes.saturating_mul(2)));
            quoted.push('"');
            quoted.push('"');
        } else {
            quoted.extend(std::iter::repeat_n('\\', backslashes));
            if character == '%' {
                // A zero-length command-extension expansion prevents cmd.exe
                // from treating the value as an environment-variable name.
                quoted.push_str("%%cd:~,");
            }
            quoted.push(character);
        }
        backslashes = 0;
    }
    if needs_quotes {
        quoted.extend(std::iter::repeat_n('\\', backslashes.saturating_mul(2)));
        quoted.push('"');
    } else {
        quoted.extend(std::iter::repeat_n('\\', backslashes));
    }
    Ok(quoted)
}
