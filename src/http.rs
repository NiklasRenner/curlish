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

    // Append query params to URL
    let url = if req.query_params.is_empty() {
        url
    } else {
        let params: Vec<String> = req
            .query_params
            .iter()
            .map(|p| {
                let k = resolve_placeholders(&p.key, env_vars);
                let v = resolve_placeholders(&p.value, env_vars);
                format!("{}={}", urlencoding(&k), urlencoding(&v))
            })
            .collect();
        let sep = if url.contains('?') { "&" } else { "?" };
        format!("{url}{sep}{}", params.join("&"))
    };
    let headers: Vec<HeaderEntry> = req
        .headers
        .iter()
        .map(|h| HeaderEntry {
            name: resolve_placeholders(&h.name, env_vars),
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

/// Minimal percent-encoding for query param keys and values.
fn urlencoding(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for b in input.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            _ => {
                out.push_str(&format!("%{b:02X}"));
            }
        }
    }
    out
}

