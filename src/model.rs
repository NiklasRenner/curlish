use serde::{Deserialize, Serialize};
use std::fmt;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiArea {
    RequestList,
    Details,
    Response,
}
impl UiArea {
    /// Move right in the layout (Requests -> Details)
    pub fn right(self) -> Self {
        match self {
            UiArea::RequestList => UiArea::Details,
            other => other,
        }
    }

    /// Move left in the layout (Details -> Requests)
    pub fn left(self) -> Self {
        match self {
            UiArea::Details => UiArea::RequestList,
            other => other,
        }
    }

    /// Move down in the layout (top row -> Response)
    pub fn down(self) -> Self {
        match self {
            UiArea::RequestList | UiArea::Details => UiArea::Response,
            other => other,
        }
    }

    /// Move up in the layout (Response -> RequestList)
    pub fn up(self) -> Self {
        match self {
            UiArea::Response => UiArea::RequestList,
            other => other,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            UiArea::RequestList => "Requests",
            UiArea::Details => "Details",
            UiArea::Response => "Response",
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestStore {
    pub requests: Vec<Request>,
}
impl Default for RequestStore {
    fn default() -> Self {
        Self {
            requests: vec![Request::sample()],
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Request {
    pub id: u64,
    pub name: String,
    pub method: HttpMethod,
    pub url: String,
    pub headers: Vec<HeaderEntry>,
    pub body: String,
}
impl Request {
    pub fn sample() -> Self {
        Self {
            id: 1,
            name: String::from("Example GET"),
            method: HttpMethod::Get,
            url: String::from("https://httpbin.org/get"),
            headers: Vec::new(),
            body: String::new(),
        }
    }
    pub fn new(id: u64) -> Self {
        Self {
            id,
            name: format!("Request {id}"),
            method: HttpMethod::Get,
            url: String::new(),
            headers: Vec::new(),
            body: String::new(),
        }
    }
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum HttpMethod {
    Get,
    Post,
    Put,
    Patch,
    Delete,
    Head,
    Options,
}
impl HttpMethod {
    pub const ALL: &'static [HttpMethod] = &[
        Self::Get, Self::Post, Self::Put, Self::Patch, Self::Delete, Self::Head, Self::Options,
    ];

    pub fn index(self) -> usize {
        Self::ALL.iter().position(|&m| m == self).unwrap_or(0)
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Get => "GET",
            Self::Post => "POST",
            Self::Put => "PUT",
            Self::Patch => "PATCH",
            Self::Delete => "DELETE",
            Self::Head => "HEAD",
            Self::Options => "OPTIONS",
        }
    }
    pub fn parse(input: &str) -> Option<Self> {
        match input.trim().to_ascii_uppercase().as_str() {
            "GET" => Some(Self::Get),
            "POST" => Some(Self::Post),
            "PUT" => Some(Self::Put),
            "PATCH" => Some(Self::Patch),
            "DELETE" => Some(Self::Delete),
            "HEAD" => Some(Self::Head),
            "OPTIONS" => Some(Self::Options),
            _ => None,
        }
    }
}
impl fmt::Display for HttpMethod {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeaderEntry {
    pub name: String,
    pub value: String,
}
#[derive(Debug, Clone)]
pub struct ResponseSummary {
    pub status: String,
    pub headers: Vec<HeaderEntry>,
    pub body: String,
}
pub fn format_headers(headers: &[HeaderEntry]) -> String {
    if headers.is_empty() {
        return String::from("(none)");
    }
    headers
        .iter()
        .map(|h| format!("{}: {}", h.name, h.value))
        .collect::<Vec<_>>()
        .join("; ")
}
pub fn parse_headers(input: &str) -> Vec<HeaderEntry> {
    input
        .split(';')
        .filter_map(|pair| {
            let mut parts = pair.trim().splitn(2, ':');
            let name = parts.next()?.trim();
            if name.is_empty() {
                return None;
            }
            let value = parts.next().unwrap_or("").trim();
            Some(HeaderEntry {
                name: String::from(name),
                value: String::from(value),
            })
        })
        .collect()
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parse_headers_splits_pairs() {
        let headers = parse_headers("Accept: application/json; X-Test: 123");
        assert_eq!(headers.len(), 2);
        assert_eq!(headers[0].name, "Accept");
        assert_eq!(headers[0].value, "application/json");
        assert_eq!(headers[1].name, "X-Test");
    }
    #[test]
    fn parse_method_is_case_insensitive() {
        assert_eq!(HttpMethod::parse("post"), Some(HttpMethod::Post));
        assert_eq!(HttpMethod::parse(" PATCH "), Some(HttpMethod::Patch));
        assert_eq!(HttpMethod::parse("nope"), None);
    }
}
