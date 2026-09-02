use std::time::Duration;

use anyhow::{Context, Result};
use serde_json::Value;

use crate::io::{MAX_HTTP_BODY_BYTES, read_capped};

pub trait JsonHttp: Send + Sync {
    fn get_json(&self, url: &str, headers: &[(&str, &str)], timeout: Duration) -> Result<Value>;

    fn post_json(
        &self,
        _url: &str,
        _headers: &[(&str, &str)],
        _body: &Value,
        _timeout: Duration,
    ) -> Result<Value> {
        anyhow::bail!("JSON POST is unavailable for this HTTP adapter")
    }
}

#[derive(Debug, Default)]
pub(crate) struct ReqwestHttp;

impl JsonHttp for ReqwestHttp {
    fn get_json(&self, url: &str, headers: &[(&str, &str)], timeout: Duration) -> Result<Value> {
        let client = reqwest::blocking::Client::builder()
            .timeout(timeout)
            .build()
            .context("build quota HTTP client")?;
        let request = headers
            .iter()
            .fold(client.get(url), |request, (name, value)| {
                request.header(*name, *value)
            });
        let mut response = request
            .send()
            .with_context(|| format!("GET {url}"))?
            .error_for_status()
            .with_context(|| format!("quota response from {url}"))?;
        if response
            .content_length()
            .is_some_and(|length| length > MAX_HTTP_BODY_BYTES as u64)
        {
            anyhow::bail!("quota response exceeds the local read limit");
        }
        let (bytes, exceeded) = read_capped(&mut response, MAX_HTTP_BODY_BYTES)
            .with_context(|| format!("read quota response from {url}"))?;
        if exceeded {
            anyhow::bail!("quota response exceeds the local read limit");
        }
        serde_json::from_slice(&bytes).with_context(|| format!("decode quota response from {url}"))
    }

    fn post_json(
        &self,
        url: &str,
        headers: &[(&str, &str)],
        body: &Value,
        timeout: Duration,
    ) -> Result<Value> {
        let client = reqwest::blocking::Client::builder()
            .timeout(timeout)
            .build()
            .context("build quota HTTP client")?;
        let request = headers
            .iter()
            .fold(client.post(url).json(body), |request, (name, value)| {
                request.header(*name, *value)
            });
        let mut response = request
            .send()
            .with_context(|| format!("POST {url}"))?
            .error_for_status()
            .with_context(|| format!("quota response from {url}"))?;
        if response
            .content_length()
            .is_some_and(|length| length > MAX_HTTP_BODY_BYTES as u64)
        {
            anyhow::bail!("quota response exceeds the local read limit");
        }
        let (bytes, exceeded) = read_capped(&mut response, MAX_HTTP_BODY_BYTES)
            .with_context(|| format!("read quota response from {url}"))?;
        if exceeded {
            anyhow::bail!("quota response exceeds the local read limit");
        }
        serde_json::from_slice(&bytes).with_context(|| format!("decode quota response from {url}"))
    }
}
