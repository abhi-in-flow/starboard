use std::sync::OnceLock;

use log::{Level, LevelFilter, Log, Metadata, Record};
use regex::Regex;

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

/// Format a `log` record and scrub secrets before the line reaches any sink.
pub fn format_record(record: &Record<'_>) -> String {
    scrub_log_line(&format!(
        "[{}] {}: {}",
        record.level(),
        record.target(),
        record.args()
    ))
}

struct ScrubbedLogger;

impl Log for ScrubbedLogger {
    fn enabled(&self, metadata: &Metadata<'_>) -> bool {
        metadata.level()
            <= if cfg!(debug_assertions) {
                Level::Debug
            } else {
                Level::Info
            }
    }

    fn log(&self, record: &Record<'_>) {
        if !self.enabled(record.metadata()) {
            return;
        }
        let line = format_record(record);
        let _ = std::io::Write::write_all(&mut std::io::stderr(), format!("{line}\n").as_bytes());
    }

    fn flush(&self) {}
}

static LOGGER: ScrubbedLogger = ScrubbedLogger;

/// Install the scrubbed stderr logger and a panic hook that never prints raw PATs.
/// Safe to call more than once.
pub fn init_logger() {
    static INIT: OnceLock<()> = OnceLock::new();
    INIT.get_or_init(|| {
        let max = if cfg!(debug_assertions) {
            LevelFilter::Debug
        } else {
            LevelFilter::Info
        };
        if log::set_logger(&LOGGER).is_ok() {
            log::set_max_level(max);
        }
        install_panic_hook();
    });
}

fn install_panic_hook() {
    std::panic::set_hook(Box::new(|info| {
        let scrubbed = scrub_log_line(&info.to_string());
        log::error!("panic: {scrubbed}");
    }));
}

/// Timing traces stay Debug so they are off in release (`LevelFilter::Info`).
pub fn debug_timing(line: impl AsRef<str>) {
    log::debug!("{}", line.as_ref());
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

    #[test]
    fn format_record_scrubs_pat_in_args() {
        let record = Record::builder()
            .args(format_args!(
                "Authorization: Bearer ghp_supersecrettoken123 extra"
            ))
            .level(Level::Info)
            .target("starboard::sync")
            .build();
        let line = format_record(&record);
        assert!(line.starts_with("[INFO] starboard::sync:"));
        assert!(
            !line.contains("ghp_supersecrettoken123"),
            "token leaked via logger format: {line}"
        );
        assert!(line.contains("[REDACTED]"));
    }

    #[test]
    fn panic_payload_is_scrubbed() {
        let raw = "panicked at Authorization: Bearer ghp_supersecrettoken123";
        let scrubbed = scrub_log_line(raw);
        assert!(!scrubbed.contains("ghp_supersecrettoken123"));
        assert!(scrubbed.contains("[REDACTED]"));
    }
}
