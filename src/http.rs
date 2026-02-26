use crate::model::{HeaderEntry, HttpMethod, Request, ResponseSummary};
use anyhow::{Context, Result};
use reqwest::blocking::Client;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};

const MAX_BODY_CHARS: usize = 64 * 1024;

pub fn execute_request(req: &Request) -> Result<ResponseSummary> {
    let client = Client::builder()
        .user_agent("curlish/0.1")
        .build()
        .context("Failed to build HTTP client")?;

    let mut builder = match req.method {
        HttpMethod::Get => client.get(&req.url),
        HttpMethod::Post => client.post(&req.url),
        HttpMethod::Put => client.put(&req.url),
        HttpMethod::Patch => client.patch(&req.url),
        HttpMethod::Delete => client.delete(&req.url),
        HttpMethod::Head => client.head(&req.url),
        HttpMethod::Options => client.request(reqwest::Method::OPTIONS, &req.url),
    };

    let headers = build_headermap(&req.headers)?;
    if !headers.is_empty() {
        builder = builder.headers(headers);
    }
    if !req.body.trim().is_empty() && req.method != HttpMethod::Get {
        builder = builder.body(req.body.clone());
    }

    let resp = builder.send().context("Request failed")?;

    let status = format!("{} {}", resp.status().as_u16(), resp.status());
    let headers = resp
        .headers()
        .iter()
        .map(|(k, v)| HeaderEntry {
            name: k.to_string(),
            value: v.to_str().unwrap_or("<binary>").into(),
        })
        .collect();
    let body = resp.text().unwrap_or_else(|_| "<failed to read body>".into());

    Ok(ResponseSummary {
        status,
        headers,
        body: truncate(body),
    })
}

fn build_headermap(entries: &[HeaderEntry]) -> Result<HeaderMap> {
    let mut map = HeaderMap::with_capacity(entries.len());
    for e in entries {
        let name = HeaderName::from_bytes(e.name.trim().as_bytes())
            .with_context(|| format!("Invalid header name: {}", e.name))?;
        let value = HeaderValue::from_str(e.value.trim())
            .with_context(|| format!("Invalid header value for {}", e.name))?;
        map.insert(name, value);
    }
    Ok(map)
}

fn truncate(mut body: String) -> String {
    if body.len() <= MAX_BODY_CHARS {
        return body;
    }

    body.truncate(MAX_BODY_CHARS);
    body.push_str("\n...<truncated>");
    body
}

