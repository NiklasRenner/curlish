use serde::{Deserialize, Serialize};
use std::fmt;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiArea {
    Environment,
    RequestList,
    Details,
    Response,
}
impl UiArea {
    /// Move right in the layout
    pub fn right(self) -> Self {
        match self {
            UiArea::Environment => UiArea::Details,
            UiArea::RequestList => UiArea::Details,
            other => other,
        }
    }

    /// Move left in the layout
    pub fn left(self) -> Self {
        match self {
            UiArea::Details => UiArea::RequestList,
            other => other,
        }
    }

    /// Move down in the layout
    pub fn down(self) -> Self {
        match self {
            UiArea::Environment => UiArea::RequestList,
            UiArea::RequestList | UiArea::Details => UiArea::Response,
            other => other,
        }
    }

    /// Move up in the layout
    pub fn up(self) -> Self {
        match self {
            UiArea::Response => UiArea::RequestList,
            UiArea::RequestList => UiArea::Environment,
            other => other,
        }
    }

    #[cfg(test)]
    pub fn label(self) -> &'static str {
        match self {
            UiArea::Environment => "Environment",
            UiArea::RequestList => "Requests",
            UiArea::Details => "Details",
            UiArea::Response => "Response",
        }
    }
}
// ── Environment ───────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Environment {
    pub name: String,
    pub variables: Vec<EnvVariable>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvVariable {
    pub key: String,
    pub value: String,
}

impl Environment {
    pub fn new(name: &str) -> Self {
        Self {
            name: String::from(name),
            variables: Vec::new(),
        }
    }
}

/// Replace `${key}` placeholders in a string using the given variables.
pub fn resolve_placeholders(input: &str, vars: &[EnvVariable]) -> String {
    let mut result = input.to_string();
    for var in vars {
        let placeholder = format!("${{{}}}", var.key);
        result = result.replace(&placeholder, &var.value);
    }
    result
}

// ── Data models ───────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestStore {
    pub requests: Vec<Request>,
    #[serde(default)]
    pub environments: Vec<Environment>,
    #[serde(default)]
    pub active_environment: Option<usize>,
}
impl Default for RequestStore {
    fn default() -> Self {
        Self {
            requests: vec![Request::sample()],
            environments: Vec::new(),
            active_environment: None,
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
    #[cfg(test)]
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
#[cfg(test)]
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

    // ── Header parsing ────────────────────────────────────────────

    #[test]
    fn parse_headers_splits_pairs() {
        let headers = parse_headers("Accept: application/json; X-Test: 123");
        assert_eq!(headers.len(), 2);
        assert_eq!(headers[0].name, "Accept");
        assert_eq!(headers[0].value, "application/json");
        assert_eq!(headers[1].name, "X-Test");
    }

    #[test]
    fn parse_headers_empty_input() {
        let headers = parse_headers("");
        assert!(headers.is_empty());
    }

    #[test]
    fn parse_headers_whitespace_only() {
        let headers = parse_headers("  ;  ;  ");
        assert!(headers.is_empty());
    }

    #[test]
    fn parse_headers_missing_value() {
        let headers = parse_headers("X-Solo");
        assert_eq!(headers.len(), 1);
        assert_eq!(headers[0].name, "X-Solo");
        assert_eq!(headers[0].value, "");
    }

    // ── HttpMethod ────────────────────────────────────────────────

    #[test]
    fn parse_method_is_case_insensitive() {
        assert_eq!(HttpMethod::parse("post"), Some(HttpMethod::Post));
        assert_eq!(HttpMethod::parse(" PATCH "), Some(HttpMethod::Patch));
        assert_eq!(HttpMethod::parse("nope"), None);
    }

    #[test]
    fn method_all_contains_every_variant() {
        assert_eq!(HttpMethod::ALL.len(), 7);
        assert!(HttpMethod::ALL.contains(&HttpMethod::Get));
        assert!(HttpMethod::ALL.contains(&HttpMethod::Options));
    }

    #[test]
    fn method_index_roundtrips() {
        for (i, &m) in HttpMethod::ALL.iter().enumerate() {
            assert_eq!(m.index(), i);
        }
    }

    #[test]
    fn method_display_matches_as_str() {
        for &m in HttpMethod::ALL {
            assert_eq!(m.to_string(), m.as_str());
        }
    }

    // ── format_headers ────────────────────────────────────────────

    #[test]
    fn format_headers_empty_shows_none() {
        assert_eq!(format_headers(&[]), "(none)");
    }

    #[test]
    fn format_headers_single() {
        let headers = vec![HeaderEntry {
            name: String::from("Host"),
            value: String::from("example.com"),
        }];
        assert_eq!(format_headers(&headers), "Host: example.com");
    }

    #[test]
    fn format_headers_multiple_joined_by_semicolon() {
        let headers = vec![
            HeaderEntry { name: String::from("A"), value: String::from("1") },
            HeaderEntry { name: String::from("B"), value: String::from("2") },
        ];
        assert_eq!(format_headers(&headers), "A: 1; B: 2");
    }

    // ── UiArea navigation ─────────────────────────────────────────

    #[test]
    fn ui_area_right_from_request_list() {
        assert_eq!(UiArea::RequestList.right(), UiArea::Details);
    }

    #[test]
    fn ui_area_right_from_environment() {
        assert_eq!(UiArea::Environment.right(), UiArea::Details);
    }

    #[test]
    fn ui_area_right_from_details_stays() {
        assert_eq!(UiArea::Details.right(), UiArea::Details);
    }

    #[test]
    fn ui_area_left_from_details() {
        assert_eq!(UiArea::Details.left(), UiArea::RequestList);
    }

    #[test]
    fn ui_area_left_from_request_list_stays() {
        assert_eq!(UiArea::RequestList.left(), UiArea::RequestList);
    }

    #[test]
    fn ui_area_down_from_top_row() {
        assert_eq!(UiArea::RequestList.down(), UiArea::Response);
        assert_eq!(UiArea::Details.down(), UiArea::Response);
    }

    #[test]
    fn ui_area_down_from_environment() {
        assert_eq!(UiArea::Environment.down(), UiArea::RequestList);
    }

    #[test]
    fn ui_area_down_from_response_stays() {
        assert_eq!(UiArea::Response.down(), UiArea::Response);
    }

    #[test]
    fn ui_area_up_from_response() {
        assert_eq!(UiArea::Response.up(), UiArea::RequestList);
    }

    #[test]
    fn ui_area_up_from_request_list() {
        assert_eq!(UiArea::RequestList.up(), UiArea::Environment);
    }

    #[test]
    fn ui_area_up_from_environment_stays() {
        assert_eq!(UiArea::Environment.up(), UiArea::Environment);
    }

    #[test]
    fn ui_area_labels_not_empty() {
        assert!(!UiArea::Environment.label().is_empty());
        assert!(!UiArea::RequestList.label().is_empty());
        assert!(!UiArea::Details.label().is_empty());
        assert!(!UiArea::Response.label().is_empty());
    }

    // ── Request constructors ──────────────────────────────────────

    #[test]
    fn request_new_has_empty_fields() {
        let req = Request::new(42);
        assert_eq!(req.id, 42);
        assert_eq!(req.method, HttpMethod::Get);
        assert!(req.url.is_empty());
        assert!(req.headers.is_empty());
        assert!(req.body.is_empty());
    }

    #[test]
    fn request_sample_has_url() {
        let req = Request::sample();
        assert!(!req.url.is_empty());
        assert_eq!(req.method, HttpMethod::Get);
    }

    #[test]
    fn request_store_default_not_empty() {
        let store = RequestStore::default();
        assert!(!store.requests.is_empty());
        assert!(store.environments.is_empty());
        assert!(store.active_environment.is_none());
    }

    // ── Environment & placeholders ─────────────────────────────────

    #[test]
    fn resolve_placeholders_replaces_variables() {
        let vars = vec![
            EnvVariable { key: String::from("host"), value: String::from("example.com") },
            EnvVariable { key: String::from("token"), value: String::from("abc123") },
        ];
        let result = resolve_placeholders("https://${host}/api?t=${token}", &vars);
        assert_eq!(result, "https://example.com/api?t=abc123");
    }

    #[test]
    fn resolve_placeholders_no_vars_unchanged() {
        let result = resolve_placeholders("https://example.com", &[]);
        assert_eq!(result, "https://example.com");
    }

    #[test]
    fn resolve_placeholders_missing_var_unchanged() {
        let vars = vec![
            EnvVariable { key: String::from("host"), value: String::from("example.com") },
        ];
        let result = resolve_placeholders("${host}/${missing}", &vars);
        assert_eq!(result, "example.com/${missing}");
    }

    #[test]
    fn environment_new_has_empty_vars() {
        let env = Environment::new("test");
        assert_eq!(env.name, "test");
        assert!(env.variables.is_empty());
    }
}
