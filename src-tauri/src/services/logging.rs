use regex::Regex;
use std::sync::OnceLock;

/// Redact Authorization headers (and similar bearer tokens) from log lines.
/// Wired into the app logger in a later phase; kept public so the scrubber test stays meaningful.
#[allow(dead_code)]
pub fn scrub_log_line(line: &str) -> String {
    static AUTH_RE: OnceLock<Option<Regex>> = OnceLock::new();
    let re = AUTH_RE.get_or_init(|| {
        Regex::new(r"(?i)(authorization\s*[:=]\s*bearer\s+)(\S+)").ok()
    });
    match re.as_ref() {
        Some(re) => re.replace_all(line, "${1}[REDACTED]").into_owned(),
        None => line.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacts_authorization_bearer_header() {
        let line = "GET /user headers={Authorization: Bearer ghp_supersecrettoken123}";
        let scrubbed = scrub_log_line(line);
        assert!(
            scrubbed.contains("Authorization: Bearer [REDACTED]"),
            "expected redacted header, got: {scrubbed}"
        );
        assert!(
            !scrubbed.contains("ghp_supersecrettoken123"),
            "token leaked: {scrubbed}"
        );
    }

    #[test]
    fn leaves_unrelated_lines_alone() {
        let line = "sync complete: 42 repos";
        assert_eq!(scrub_log_line(line), line);
    }
}
