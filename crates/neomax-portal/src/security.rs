use std::fmt::Display;

use anyhow::{Result, bail};

use crate::address::LocalBind;
use crate::http::HttpRequest;

const MAX_DIAGNOSTIC_BYTES: usize = 512;

pub(crate) fn require_loopback_host(request: &HttpRequest, bind: LocalBind) -> Result<()> {
    let value = request
        .headers
        .get("host")
        .ok_or_else(|| anyhow::anyhow!("Host header is required"))?;
    let (host, port) = split_host(value)?;
    let valid_host = host.eq_ignore_ascii_case("localhost") || matches!(host, "127.0.0.1" | "::1");
    if !valid_host || port.is_some_and(|port| port != bind.port()) {
        bail!("Host header is not a permitted loopback address")
    }
    Ok(())
}

pub(crate) fn require_json_content_type(request: &HttpRequest) -> Result<()> {
    let value = request
        .headers
        .get("content-type")
        .ok_or_else(|| anyhow::anyhow!("JSON Content-Type is required"))?;
    let media_type = value.split(';').next().map(str::trim).unwrap_or_default();
    if !media_type.eq_ignore_ascii_case("application/json") {
        bail!("JSON Content-Type is required")
    }
    Ok(())
}

pub(crate) fn log_internal(context: &str, error: &(impl Display + ?Sized)) {
    eprintln!(
        "neomax-portal {context}: {}",
        safe_diagnostic(&error.to_string())
    );
}

pub(crate) fn safe_diagnostic(value: &str) -> String {
    let mut output = String::with_capacity(value.len().min(MAX_DIAGNOSTIC_BYTES));
    let mut redacted = false;
    for word in value.split_whitespace() {
        let lower = word.to_ascii_lowercase();
        let sensitive = lower.contains("token")
            || lower.contains("secret")
            || lower.contains("password")
            || lower.contains("credential")
            || lower.contains("authorization")
            || lower.contains("bearer")
            || lower.contains("oauth")
            || lower.contains("/users/")
            || lower.contains("/home/")
            || lower.contains("/private/")
            || lower.contains("/var/")
            || lower.contains("c:\\")
            || lower.contains("d:\\");
        if sensitive {
            if !redacted {
                output.push_str("[redacted]");
                redacted = true;
            }
        } else {
            if !output.is_empty() {
                output.push(' ');
            }
            output.push_str(word);
        }
        if output.len() >= MAX_DIAGNOSTIC_BYTES {
            output.truncate(MAX_DIAGNOSTIC_BYTES);
            break;
        }
    }
    if output.is_empty() {
        "internal portal error".into()
    } else {
        output
    }
}

fn split_host(value: &str) -> Result<(&str, Option<u16>)> {
    if value.is_empty() || value.chars().any(char::is_whitespace) {
        bail!("Host header is invalid")
    }
    if let Some(rest) = value.strip_prefix('[') {
        let end = rest
            .find(']')
            .ok_or_else(|| anyhow::anyhow!("Host header is invalid"))?;
        let host = &rest[..end];
        let suffix = &rest[end + 1..];
        let port = parse_optional_port(suffix)?;
        if host != "::1" {
            bail!("Host header is invalid")
        }
        return Ok((host, port));
    }
    if value.matches(':').count() > 1 {
        bail!("IPv6 Host headers must be bracketed")
    }
    if let Some((host, raw_port)) = value.split_once(':') {
        if host.is_empty() {
            bail!("Host header is invalid")
        }
        return Ok((host, Some(parse_port(raw_port)?)));
    }
    Ok((value, None))
}

fn parse_optional_port(value: &str) -> Result<Option<u16>> {
    if value.is_empty() {
        Ok(None)
    } else if let Some(raw_port) = value.strip_prefix(':') {
        Ok(Some(parse_port(raw_port)?))
    } else {
        bail!("Host header is invalid")
    }
}

fn parse_port(value: &str) -> Result<u16> {
    if value.is_empty() {
        bail!("Host header is invalid")
    }
    let port = value
        .parse::<u16>()
        .map_err(|_| anyhow::anyhow!("Host header is invalid"))?;
    if port == 0 {
        bail!("Host header is invalid")
    }
    Ok(port)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;

    fn request(host: Option<&str>) -> HttpRequest {
        let mut headers = BTreeMap::new();
        if let Some(host) = host {
            headers.insert("host".into(), host.into());
        }
        HttpRequest {
            method: "GET".into(),
            target: "/".into(),
            path: "/".into(),
            query: BTreeMap::new(),
            headers,
            body: Vec::new(),
        }
    }

    #[test]
    fn accepts_loopback_host_forms_with_no_port_or_bound_port() {
        let bind = LocalBind::loopback(8787);
        for host in [
            "localhost",
            "localhost:8787",
            "127.0.0.1",
            "127.0.0.1:8787",
            "[::1]",
            "[::1]:8787",
        ] {
            assert!(
                require_loopback_host(&request(Some(host)), bind).is_ok(),
                "{host}"
            );
        }
    }

    #[test]
    fn rejects_dns_rebinding_and_wrong_port_forms() {
        let bind = LocalBind::loopback(8787);
        for host in [
            "localhost.evil.test",
            "127.0.0.1.evil.test",
            "evil.test",
            "localhost:8788",
            "127.0.0.1:80",
            "[::1]:8788",
            "::1",
            "[::1].evil.test",
            "127.000.000.001",
        ] {
            assert!(
                require_loopback_host(&request(Some(host)), bind).is_err(),
                "{host}"
            );
        }
        assert!(require_loopback_host(&request(None), bind).is_err());
    }

    #[test]
    fn content_type_requires_json_media_type() {
        let mut request = request(Some("localhost:8787"));
        assert!(require_json_content_type(&request).is_err());
        request
            .headers
            .insert("content-type".into(), "text/plain".into());
        assert!(require_json_content_type(&request).is_err());
        request.headers.insert(
            "content-type".into(),
            "application/json; charset=utf-8".into(),
        );
        assert!(require_json_content_type(&request).is_ok());
    }

    #[test]
    fn diagnostics_redact_sensitive_values_and_bound_output() {
        let value = safe_diagnostic(
            "failed /private/fixture/project token=secret bearer abc /state with a very long tail",
        );
        assert!(!value.contains("/private/fixture"));
        assert!(!value.contains("secret"));
        assert!(!value.contains("bearer"));
        assert!(value.len() <= MAX_DIAGNOSTIC_BYTES);
    }
}
