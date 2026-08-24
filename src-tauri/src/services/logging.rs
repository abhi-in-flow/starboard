use regex::Regex;
use std::sync::OnceLock;

/// Redact Authorization headers, bearer tokens, and common GitHub PAT shapes.
pub fn scrub_log_line(line: &str) -> String {
    static AUTH_RE: OnceLock<Option<Regex>> = OnceLock::new();
    static TOKEN_RE: OnceLock<Option<Regex>> = OnceLock::new();
    let auth = AUTH_RE
        .get_or_init(|| Regex::new(r"(?i)(authorization\s*[:=]\s*(?:bearer|token)\s+)(\S+)").ok());
    let tokens = TOKEN_RE.get_or_init(|| {
        Regex::new(
            r"(?i)\b((?:ghp|gho|ghu|ghs|ghr)_[A-Za-z0-9_]{20,}|github_pat_[A-Za-z0-9_]{20,})\b",
        )
        .ok()
    });
    let mut out = match auth.as_ref() {
        Some(re) => re.replace_all(line, "${1}[REDACTED]").into_owned(),
        None => line.to_string(),
    };
    if let Some(re) = tokens.as_ref() {
        out = re.replace_all(&out, "[REDACTED]").into_owned();
    }
    out
}

/// Single sink for runtime logs. Every line is scrubbed before it reaches stderr.
pub fn log_line(line: impl AsRef<str>) {
    eprintln!("{}", scrub_log_line(line.as_ref()));
}

/// Timing traces stay off in release builds so they cannot leak into shipped logs.
pub fn debug_timing(line: impl AsRef<str>) {
    if cfg!(debug_assertions) {
        log_line(line);
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
    fn redacts_raw_pat_shapes() {
        let line =
            "stored github_pat_abcdefghijklmnopqrstuvwxyz123456 and ghp_abcdefghijklmnopqrstuv";
        let scrubbed = scrub_log_line(line);
        assert!(!scrubbed.contains("github_pat_"));
        assert!(!scrubbed.contains("ghp_abcdefgh"));
        assert!(scrubbed.contains("[REDACTED]"));
    }

    #[test]
    fn leaves_unrelated_lines_alone() {
        let line = "sync complete: 42 repos";
        assert_eq!(scrub_log_line(line), line);
    }
}
