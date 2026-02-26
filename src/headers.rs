/// Common HTTP header names and their typical values for autocomplete.

pub const COMMON_HEADER_NAMES: &[&str] = &[
    "Accept",
    "Accept-Charset",
    "Accept-Encoding",
    "Accept-Language",
    "Authorization",
    "Cache-Control",
    "Connection",
    "Content-Disposition",
    "Content-Encoding",
    "Content-Length",
    "Content-Type",
    "Cookie",
    "DNT",
    "Date",
    "ETag",
    "Expect",
    "Forwarded",
    "From",
    "Host",
    "If-Match",
    "If-Modified-Since",
    "If-None-Match",
    "If-Range",
    "If-Unmodified-Since",
    "Keep-Alive",
    "Link",
    "Location",
    "Origin",
    "Pragma",
    "Proxy-Authorization",
    "Range",
    "Referer",
    "Retry-After",
    "Server",
    "Set-Cookie",
    "TE",
    "Transfer-Encoding",
    "Upgrade",
    "User-Agent",
    "Vary",
    "Via",
    "WWW-Authenticate",
    "X-Forwarded-For",
    "X-Forwarded-Host",
    "X-Forwarded-Proto",
    "X-Request-Id",
    "X-Requested-With",
];

/// Given a header name, return common values for autocomplete.
pub fn common_values_for(header_name: &str) -> &'static [&'static str] {
    match header_name.to_ascii_lowercase().as_str() {
        "content-type" => &[
            "application/json",
            "application/x-www-form-urlencoded",
            "application/xml",
            "application/octet-stream",
            "multipart/form-data",
            "text/plain",
            "text/html",
            "text/css",
            "text/xml",
        ],
        "accept" => &[
            "application/json",
            "application/xml",
            "text/html",
            "text/plain",
            "*/*",
        ],
        "accept-encoding" => &[
            "gzip",
            "deflate",
            "br",
            "gzip, deflate, br",
            "identity",
        ],
        "accept-charset" => &[
            "utf-8",
            "iso-8859-1",
            "utf-8, iso-8859-1;q=0.5",
        ],
        "accept-language" => &[
            "en-US",
            "en-US,en;q=0.9",
            "en-GB",
            "de-DE",
            "fr-FR",
            "*",
        ],
        "authorization" => &[
            "Bearer ",
            "Basic ",
        ],
        "cache-control" => &[
            "no-cache",
            "no-store",
            "max-age=0",
            "max-age=3600",
            "public",
            "private",
        ],
        "connection" => &[
            "keep-alive",
            "close",
        ],
        "content-encoding" => &[
            "gzip",
            "deflate",
            "br",
            "identity",
        ],
        "transfer-encoding" => &[
            "chunked",
            "compress",
            "deflate",
            "gzip",
        ],
        "x-requested-with" => &[
            "XMLHttpRequest",
        ],
        _ => &[],
    }
}

/// Filter a list of suggestions by a prefix (case-insensitive).
pub fn filter_suggestions<'a>(suggestions: &'a [&'a str], input: &str) -> Vec<&'a str> {
    if input.is_empty() {
        return suggestions.to_vec();
    }
    let lower = input.to_ascii_lowercase();
    suggestions
        .iter()
        .copied()
        .filter(|s| s.to_ascii_lowercase().contains(&lower))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filter_empty_input_returns_all() {
        let result = filter_suggestions(COMMON_HEADER_NAMES, "");
        assert_eq!(result.len(), COMMON_HEADER_NAMES.len());
    }

    #[test]
    fn filter_narrows_by_substring() {
        let result = filter_suggestions(COMMON_HEADER_NAMES, "content");
        assert!(result.contains(&"Content-Type"));
        assert!(result.contains(&"Content-Length"));
        assert!(result.contains(&"Content-Encoding"));
        assert!(!result.contains(&"Accept"));
    }

    #[test]
    fn filter_is_case_insensitive() {
        let result = filter_suggestions(COMMON_HEADER_NAMES, "ACCEPT");
        assert!(result.contains(&"Accept"));
        assert!(result.contains(&"Accept-Encoding"));
    }

    #[test]
    fn filter_no_match_returns_empty() {
        let result = filter_suggestions(COMMON_HEADER_NAMES, "zzzzz");
        assert!(result.is_empty());
    }

    #[test]
    fn common_values_for_content_type() {
        let values = common_values_for("Content-Type");
        assert!(values.contains(&"application/json"));
        assert!(values.contains(&"text/html"));
    }

    #[test]
    fn common_values_case_insensitive_lookup() {
        let values = common_values_for("content-type");
        assert!(!values.is_empty());
        assert_eq!(values, common_values_for("Content-Type"));
    }

    #[test]
    fn common_values_for_unknown_header_is_empty() {
        let values = common_values_for("X-Custom-Nonsense");
        assert!(values.is_empty());
    }

    #[test]
    fn common_values_for_authorization() {
        let values = common_values_for("Authorization");
        assert!(values.contains(&"Bearer "));
        assert!(values.contains(&"Basic "));
    }

    #[test]
    fn filter_values_for_content_type() {
        let values = common_values_for("Content-Type");
        let filtered = filter_suggestions(values, "json");
        assert!(filtered.contains(&"application/json"));
        assert!(!filtered.contains(&"text/html"));
    }
}
