use regex::Regex;
use std::sync::OnceLock;

/// HTML entity escaping (JS: & < > ' " → entities), used before persisting
/// user-editable text that the frontend renders as HTML.
pub fn escape_html(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for ch in input.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '\'' => out.push_str("&#39;"),
            '"' => out.push_str("&quot;"),
            _ => out.push(ch),
        }
    }
    out
}

fn invalid_name_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?i)<|>|script|javascript:|on\w+\s*=").unwrap())
}

/// JS validator: /<|>|script|javascript:|on\w+\s*=/i for display names.
pub fn name_contains_invalid_characters(name: &str) -> bool {
    invalid_name_re().is_match(name)
}

/// JS: /^[\w-]{3,16}$/i — \w is ASCII there, so mirror it exactly.
pub fn is_valid_username(name: &str) -> bool {
    let bytes = name.as_bytes();
    (3..=16).contains(&bytes.len())
        && bytes
            .iter()
            .all(|b| b.is_ascii_alphanumeric() || *b == b'_' || *b == b'-')
}
