/// Safe schemes for README Markdown links and images.
#[must_use]
pub fn is_safe_markdown_url(raw: &str) -> bool {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return false;
    }
    if trimmed.starts_with('#') {
        return !trimmed.contains(':');
    }
    // Protocol-relative URLs are not a known safe origin in the desktop shell.
    if trimmed.starts_with("//") {
        return false;
    }
    // Path-relative links stay on GitHub pages; allow.
    if trimmed.starts_with('/') {
        return true;
    }
    let Some((scheme, rest)) = trimmed.split_once(':') else {
        // No scheme — treat as relative path.
        return !trimmed.contains("\\") && !trimmed.contains("..");
    };
    let scheme = scheme.to_ascii_lowercase();
    if matches!(scheme.as_str(), "javascript" | "data" | "file" | "vbscript") {
        return false;
    }
    if scheme != "http" && scheme != "https" {
        return false;
    }
    !rest.starts_with("//javascript") && !trimmed.to_ascii_lowercase().contains("javascript:")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allows_http_https_and_relative() {
        assert!(is_safe_markdown_url("https://github.com/o/r"));
        assert!(is_safe_markdown_url("http://192.168.1.10/docs"));
        assert!(is_safe_markdown_url("/owner/repo#readme"));
        assert!(is_safe_markdown_url("#section"));
        assert!(is_safe_markdown_url("docs/guide.md"));
    }

    #[test]
    fn blocks_dangerous_schemes() {
        assert!(!is_safe_markdown_url("javascript:alert(1)"));
        assert!(!is_safe_markdown_url("JAVASCRIPT:alert(1)"));
        assert!(!is_safe_markdown_url("data:text/html;base64,PHNjcmlwdD4="));
        assert!(!is_safe_markdown_url("file:///etc/passwd"));
        assert!(!is_safe_markdown_url("vbscript:msgbox"));
        assert!(!is_safe_markdown_url(""));
        assert!(!is_safe_markdown_url("http:javascript:alert(1)"));
        assert!(!is_safe_markdown_url(
            "https://example.com/x?next=javascript:alert(1)"
        ));
        assert!(!is_safe_markdown_url("//evil.example/payload"));
    }
}
