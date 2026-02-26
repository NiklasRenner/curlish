use crate::model::{resolve_placeholders, EnvVariable, HeaderEntry, HttpMethod, Request, ResponseSummary};
use anyhow::{Context, Result};
use reqwest::blocking::Client;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};

const MAX_BODY_CHARS: usize = 64 * 1024;

pub fn execute_request(req: &Request, env_vars: &[EnvVariable]) -> Result<ResponseSummary> {
    let client = Client::builder()
        .user_agent("curlish/0.1")
        .build()
        .context("Failed to build HTTP client")?;

    let url = resolve_placeholders(&req.url, env_vars);
    let body = resolve_placeholders(&req.body, env_vars);
    let headers: Vec<HeaderEntry> = req
        .headers
        .iter()
        .map(|h| HeaderEntry {
            name: h.name.clone(),
            value: resolve_placeholders(&h.value, env_vars),
        })
        .collect();

    let mut builder = match req.method {
        HttpMethod::Get => client.get(&url),
        HttpMethod::Post => client.post(&url),
        HttpMethod::Put => client.put(&url),
        HttpMethod::Patch => client.patch(&url),
        HttpMethod::Delete => client.delete(&url),
        HttpMethod::Head => client.head(&url),
        HttpMethod::Options => client.request(reqwest::Method::OPTIONS, &url),
    };

    let headermap = build_headermap(&headers)?;
    if !headermap.is_empty() {
        builder = builder.headers(headermap);
    }
    if !body.trim().is_empty() && req.method != HttpMethod::Get {
        builder = builder.body(body);
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

