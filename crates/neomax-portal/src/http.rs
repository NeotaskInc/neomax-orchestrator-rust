use std::collections::BTreeMap;
use std::io::{BufReader, Read, Write};
use std::net::TcpStream;

use anyhow::{Result, bail};

const MAX_REQUEST_BYTES: usize = 64 * 1024;
const MAX_BODY_BYTES: usize = 32 * 1024;
const MAX_RESPONSE_BYTES: usize = 8 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpRequest {
    pub method: String,
    pub target: String,
    pub path: String,
    pub query: BTreeMap<String, String>,
    pub headers: BTreeMap<String, String>,
    pub body: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpResponse {
    pub status: u16,
    pub content_type: &'static str,
    pub body: Vec<u8>,
}

impl HttpResponse {
    pub fn json<T: serde::Serialize>(status: u16, value: &T) -> Result<Self> {
        let body = serde_json::to_vec(value)?;
        if body.len() > MAX_RESPONSE_BYTES {
            bail!("portal response is too large")
        }
        Ok(Self {
            status,
            content_type: "application/json; charset=utf-8",
            body,
        })
    }

    pub fn text(status: u16, content_type: &'static str, body: impl Into<Vec<u8>>) -> Self {
        Self {
            status,
            content_type,
            body: body.into(),
        }
    }

    pub fn write_to(&self, stream: &mut TcpStream) -> Result<()> {
        if self.body.len() > MAX_RESPONSE_BYTES {
            bail!("portal response is too large")
        }
        let reason = reason(self.status);
        write!(
            stream,
            "HTTP/1.1 {} {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\nX-Content-Type-Options: nosniff\r\n\r\n",
            self.status,
            reason,
            self.content_type,
            self.body.len()
        )?;
        stream.write_all(&self.body)?;
        stream.flush()?;
        Ok(())
    }
}

pub fn read_request(stream: &mut TcpStream) -> Result<HttpRequest> {
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut consumed = 0usize;
    let first = read_line_bounded(&mut reader, MAX_REQUEST_BYTES)?;
    consumed += first.len();
    if consumed > MAX_REQUEST_BYTES {
        bail!("request is too large")
    }
    let first = String::from_utf8(first).map_err(|_| anyhow::anyhow!("invalid request line"))?;
    let mut parts = first.trim_end_matches(['\r', '\n']).split_whitespace();
    let method = parts
        .next()
        .ok_or_else(|| anyhow::anyhow!("missing HTTP method"))?;
    let target = parts
        .next()
        .ok_or_else(|| anyhow::anyhow!("missing HTTP target"))?;
    let version = parts
        .next()
        .ok_or_else(|| anyhow::anyhow!("missing HTTP version"))?;
    if version != "HTTP/1.0" && version != "HTTP/1.1" {
        bail!("unsupported HTTP version")
    }
    let mut headers = BTreeMap::new();
    loop {
        let line = read_line_bounded(&mut reader, MAX_REQUEST_BYTES.saturating_sub(consumed))?;
        consumed += line.len();
        if consumed > MAX_REQUEST_BYTES {
            bail!("request is too large")
        }
        let line = String::from_utf8(line).map_err(|_| anyhow::anyhow!("invalid HTTP header"))?;
        let line = line.trim_end_matches(['\r', '\n']);
        if line.is_empty() {
            break;
        }
        let (name, value) = line
            .split_once(':')
            .ok_or_else(|| anyhow::anyhow!("malformed HTTP header"))?;
        let name = name.trim().to_ascii_lowercase();
        if headers.contains_key(&name) {
            bail!("duplicate HTTP header")
        }
        headers.insert(name, value.trim().into());
    }
    if headers
        .get("transfer-encoding")
        .is_some_and(|value: &String| !value.eq_ignore_ascii_case("identity"))
    {
        bail!("chunked request bodies are not supported")
    }
    let content_length = headers
        .get("content-length")
        .map(|value| value.parse::<usize>())
        .transpose()
        .map_err(|_| anyhow::anyhow!("invalid content length"))?
        .unwrap_or(0);
    if content_length > MAX_BODY_BYTES
        || consumed.saturating_add(content_length) > MAX_REQUEST_BYTES
    {
        bail!("request body is too large")
    }
    let mut body = vec![0; content_length];
    reader.read_exact(&mut body)?;
    let (path, query) = parse_target(target)?;
    Ok(HttpRequest {
        method: method.into(),
        target: target.into(),
        path,
        query,
        headers,
        body,
    })
}

fn read_line_bounded(reader: &mut impl Read, limit: usize) -> Result<Vec<u8>> {
    let mut line = Vec::new();
    let mut byte = [0u8; 1];
    loop {
        if line.len() >= limit {
            bail!("request line or headers exceed the portal limit")
        }
        let read = reader.read(&mut byte)?;
        if read == 0 {
            if line.is_empty() {
                bail!("unexpected end of HTTP request")
            }
            break;
        }
        line.push(byte[0]);
        if byte[0] == b'\n' {
            return Ok(line);
        }
    }
    Ok(line)
}

fn parse_target(target: &str) -> Result<(String, BTreeMap<String, String>)> {
    if !target.starts_with('/') || target.len() > MAX_REQUEST_BYTES {
        bail!("HTTP target must be a local path")
    }
    let (path, raw_query) = target.split_once('?').unwrap_or((target, ""));
    let path = percent_decode(path)?;
    if path.contains('\0') || path.contains("..") {
        bail!("unsafe HTTP path")
    }
    let mut query = BTreeMap::new();
    for item in raw_query.split('&').filter(|item| !item.is_empty()) {
        let (key, value) = item.split_once('=').unwrap_or((item, ""));
        query.insert(percent_decode(key)?, percent_decode(value)?);
    }
    Ok((path, query))
}

fn percent_decode(value: &str) -> Result<String> {
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            if index + 2 >= bytes.len() {
                bail!("malformed percent escape")
            }
            let high = hex(bytes[index + 1])?;
            let low = hex(bytes[index + 2])?;
            decoded.push((high << 4) | low);
            index += 3;
        } else if bytes[index] == b'+' {
            decoded.push(b' ');
            index += 1;
        } else {
            decoded.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(decoded).map_err(|_| anyhow::anyhow!("HTTP path is not UTF-8"))
}

fn hex(value: u8) -> Result<u8> {
    match value {
        b'0'..=b'9' => Ok(value - b'0'),
        b'a'..=b'f' => Ok(value - b'a' + 10),
        b'A'..=b'F' => Ok(value - b'A' + 10),
        _ => bail!("malformed percent escape"),
    }
}

fn reason(status: u16) -> &'static str {
    match status {
        200 => "OK",
        202 => "Accepted",
        400 => "Bad Request",
        404 => "Not Found",
        405 => "Method Not Allowed",
        403 => "Forbidden",
        409 => "Conflict",
        415 => "Unsupported Media Type",
        503 => "Service Unavailable",
        413 => "Payload Too Large",
        500 => "Internal Server Error",
        _ => "Error",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;
    use std::thread;

    #[test]
    fn parses_query_and_normalizes_percent_escapes() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let client = thread::spawn(move || {
            let mut stream = TcpStream::connect(address).unwrap();
            stream
                .write_all(b"GET /api/usage?days=7&url=https%3A%2F%2Flocal HTTP/1.1\r\nHost: localhost\r\n\r\n")
                .unwrap();
        });
        let (mut stream, _) = listener.accept().unwrap();
        let request = read_request(&mut stream).unwrap();
        client.join().unwrap();
        assert_eq!(request.path, "/api/usage");
        assert_eq!(request.query["days"], "7");
        assert_eq!(request.query["url"], "https://local");
        assert!(request.body.is_empty());
    }

    #[test]
    fn rejects_traversal_and_malformed_escapes() {
        assert!(parse_target("/../secret").is_err());
        assert!(parse_target("/%ZZ").is_err());
    }

    #[test]
    fn accepts_bounded_json_body_and_rejects_slowloris_sized_headers() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let client = thread::spawn(move || {
            let mut stream = TcpStream::connect(address).unwrap();
            stream
                .write_all(
                    b"POST /api/action HTTP/1.1\r\nContent-Length: 16\r\n\r\n{\"confirm\":true}",
                )
                .unwrap();
        });
        let (mut stream, _) = listener.accept().unwrap();
        let request = read_request(&mut stream).unwrap();
        client.join().unwrap();
        assert_eq!(request.body, br#"{"confirm":true}"#);

        assert!(parse_target(&format!("/{}", "x".repeat(MAX_REQUEST_BYTES))).is_err());
    }

    #[test]
    fn rejects_oversized_request_body_before_allocating_it() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let client = thread::spawn(move || {
            let mut stream = TcpStream::connect(address).unwrap();
            stream
                .write_all(b"POST /api/action HTTP/1.1\r\nContent-Length: 40000\r\n\r\n")
                .unwrap();
        });
        let (mut stream, _) = listener.accept().unwrap();
        assert!(read_request(&mut stream).is_err());
        client.join().unwrap();
    }

    #[test]
    fn rejects_a_single_oversized_header_line() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let client = thread::spawn(move || {
            let mut stream = TcpStream::connect(address).unwrap();
            let request = format!(
                "GET / HTTP/1.1\r\nX-Long: {}\r\n\r\n",
                "x".repeat(MAX_REQUEST_BYTES)
            );
            let _ = stream.write_all(request.as_bytes());
        });
        let (mut stream, _) = listener.accept().unwrap();
        assert!(read_request(&mut stream).is_err());
        client.join().unwrap();
    }

    #[test]
    fn refuses_oversized_responses_before_writing() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let client = thread::spawn(move || TcpStream::connect(address).unwrap());
        let (mut stream, _) = listener.accept().unwrap();
        let response = HttpResponse::text(200, "text/plain", vec![b'x'; MAX_RESPONSE_BYTES + 1]);
        assert!(response.write_to(&mut stream).is_err());
        drop(stream);
        client.join().unwrap();
    }
}
