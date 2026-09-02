use std::collections::BTreeMap;

#[derive(Debug, Clone)]
pub struct Invocation {
    pub fields: BTreeMap<String, String>,
    #[allow(dead_code)]
    pub args: Vec<String>,
}

impl Invocation {
    pub fn field(&self, name: &str) -> Option<&str> {
        self.fields.get(name).map(String::as_str)
    }

    #[allow(dead_code)]
    pub fn has_arg(&self, value: &str) -> bool {
        self.args.iter().any(|arg| arg == value)
    }

    #[allow(dead_code)]
    pub fn arg_value(&self, flag: &str) -> Option<&str> {
        self.args
            .iter()
            .position(|arg| arg == flag)
            .and_then(|index| self.args.get(index + 1))
            .map(String::as_str)
    }

    #[allow(dead_code)]
    pub fn model_arg(&self) -> Option<&str> {
        self.arg_value("--model").or_else(|| self.arg_value("-m"))
    }
}

pub(super) fn parse(contents: &str) -> Vec<Invocation> {
    let blocks = if contents.contains(RECORD_END) {
        contents.split(RECORD_END).collect::<Vec<_>>()
    } else {
        contents.split("\n\n").collect::<Vec<_>>()
    };
    blocks.into_iter().filter_map(parse_block).collect()
}

const RECORD_END: &str = "__NEOMAX_E2E_RECORD_END__";

fn parse_block(block: &str) -> Option<Invocation> {
    let block = block.trim_start_matches(['\r', '\n']);
    if block.is_empty() {
        return None;
    }
    if let Some((metadata, payload)) = block.split_once("\nargs=") {
        let fields = parse_fields(metadata);
        let args = parse_legacy_args(payload);
        return fields
            .contains_key("provider")
            .then_some(Invocation { fields, args });
    }
    let mut fields = BTreeMap::new();
    let mut args = Vec::new();
    for line in block.lines() {
        if let Some(value) = line.strip_prefix("args64=") {
            args = parse_base64_args(value)?;
        } else if let Some(value) = line.strip_prefix("args=") {
            args = parse_legacy_args(value);
        } else if let Some((key, value)) = parse_field(line) {
            fields.insert(key, value);
        }
    }
    fields
        .contains_key("provider")
        .then_some(Invocation { fields, args })
}

fn parse_legacy_args(payload: &str) -> Vec<String> {
    let payload = payload
        .strip_suffix("\r\n")
        .or_else(|| payload.strip_suffix('\n'))
        .unwrap_or(payload);
    if payload.is_empty() {
        return Vec::new();
    }
    let payload = payload.strip_suffix('\u{1f}').unwrap_or(payload);
    payload.split('\u{1f}').map(str::to_owned).collect()
}

fn parse_base64_args(payload: &str) -> Option<Vec<String>> {
    let (count, encoded) = payload.split_once(':')?;
    let count = count.parse::<usize>().ok()?;
    if count == 0 {
        return encoded.is_empty().then_some(Vec::new());
    }
    let args = encoded
        .split('\u{1f}')
        .map(|value| {
            let bytes = base64_decode(value)?;
            String::from_utf8(bytes).ok()
        })
        .collect::<Option<Vec<_>>>()?;
    (args.len() == count).then_some(args)
}

fn base64_decode(value: &str) -> Option<Vec<u8>> {
    let mut bytes = Vec::with_capacity(value.len() * 3 / 4);
    let mut block = 0_u32;
    let mut length = 0;
    for byte in value.bytes() {
        let value = match byte {
            b'A'..=b'Z' => byte - b'A',
            b'a'..=b'z' => byte - b'a' + 26,
            b'0'..=b'9' => byte - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            b'=' => {
                if length < 2 {
                    return None;
                }
                break;
            }
            _ => return None,
        };
        block = (block << 6) | u32::from(value);
        length += 1;
        if length == 4 {
            bytes.extend_from_slice(&block.to_be_bytes()[1..]);
            block = 0;
            length = 0;
        }
    }
    match length {
        0 => Some(bytes),
        2 => {
            bytes.push((block >> 4) as u8);
            Some(bytes)
        }
        3 => {
            bytes.extend_from_slice(&[(block >> 10) as u8, (block >> 2) as u8]);
            Some(bytes)
        }
        _ => None,
    }
}

fn parse_fields(metadata: &str) -> BTreeMap<String, String> {
    metadata.lines().filter_map(parse_field).collect()
}

fn parse_field(line: &str) -> Option<(String, String)> {
    line.split_once('=')
        .map(|(key, value)| (key.to_owned(), value.trim_end_matches('\r').to_owned()))
}

#[cfg(test)]
mod tests {
    use super::{RECORD_END, parse};

    #[test]
    fn sentinel_framing_preserves_embedded_blank_lines() {
        let prompt = "directive\n\n\nfixture task\ntrailing";
        let contents = format!(
            "provider=kimi\nargs=--prompt\u{1f}{prompt}\u{1f}\n{RECORD_END}\nprovider=claude\nargs=--version\u{1f}\n{RECORD_END}\n"
        );

        let invocations = parse(&contents);

        assert_eq!(invocations.len(), 2);
        assert_eq!(invocations[0].args, vec!["--prompt", prompt]);
        assert_eq!(invocations[1].args, vec!["--version"]);
    }

    #[test]
    fn legacy_blank_line_records_remain_readable() {
        let invocations = parse("provider=claude\nargs=--version\u{1f}\n\n");

        assert_eq!(invocations.len(), 1);
        assert_eq!(invocations[0].args, vec!["--version"]);
    }

    #[test]
    fn windows_cmd_input_records_drop_the_extra_carriage_return() {
        let contents = format!(
            "provider=claude\r\nstdin_probe=root-stdio\r\r\nargs=--version\u{1f}\r\n{RECORD_END}\r\n"
        );

        let invocations = parse(&contents);

        assert_eq!(invocations.len(), 1);
        assert_eq!(invocations[0].field("stdin_probe"), Some("root-stdio"));
        assert_eq!(invocations[0].args, vec!["--version"]);
    }

    #[test]
    fn base64_records_preserve_empty_and_windows_sensitive_arguments() {
        let contents = format!(
            "provider=claude\nargs64=5:\u{1f}IQ==\u{1f}YSJi\u{1f}YSZifGNePCVkJQ==\u{1f}ZmluYWw=\n{RECORD_END}\n"
        );

        let invocations = parse(&contents);

        assert_eq!(
            invocations[0].args,
            vec!["", "!", "a\"b", "a&b|c^<%d%", "final"]
        );
    }

    #[test]
    fn legacy_records_preserve_doubled_quotes_without_rewriting() {
        let invocations = parse("provider=claude\nargs=a\"\"b\u{1f}\n\n");

        assert_eq!(invocations[0].args, vec!["a\"\"b"]);
    }
}
