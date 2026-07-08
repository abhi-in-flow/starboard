use std::sync::OnceLock;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use rand::Rng;
use regex::Regex;
use reqwest::header::{HeaderMap, HeaderValue, ACCEPT, AUTHORIZATION, LINK, USER_AGENT};
use reqwest::{Client, Response, StatusCode};

use crate::error::{AppError, AppResult};
use crate::models::{GitHubReadme, StarredItem, StarredRepo};

pub const STAR_ACCEPT: &str = "application/vnd.github.star+json";
pub const API_VERSION: &str = "2022-11-28";
const DEFAULT_USER_AGENT: &str = "Starboard/0.1 (+https://github.com/brocode/starboard)";
const RATE_LIMIT_FLOOR: u32 = 50;
const MAX_RETRIES: u32 = 3;

#[derive(Debug, Clone)]
pub struct GitHubClient {
    client: Client,
    base_url: String,
    pat: String,
}

#[derive(Debug, Clone)]
pub struct StarredPage {
    pub repos: Vec<StarredRepo>,
    pub next_url: Option<String>,
    pub etag: Option<String>,
    pub not_modified: bool,
}

impl GitHubClient {
    pub fn new(pat: impl Into<String>, base_url: impl Into<String>) -> AppResult<Self> {
        let client = Client::builder()
            .user_agent(DEFAULT_USER_AGENT)
            .build()?;
        Ok(Self {
            client,
            base_url: base_url.into().trim_end_matches('/').to_string(),
            pat: pat.into(),
        })
    }

    pub fn production(pat: impl Into<String>) -> AppResult<Self> {
        Self::new(pat, "https://api.github.com")
    }

    pub async fn fetch_starred_page(
        &self,
        url: Option<&str>,
        etag: Option<&str>,
    ) -> AppResult<StarredPage> {
        let request_url = url
            .map(str::to_string)
            .unwrap_or_else(|| format!("{}/user/starred?per_page=100", self.base_url));

        let mut headers = self.auth_headers()?;
        headers.insert(ACCEPT, HeaderValue::from_static(STAR_ACCEPT));
        if let Some(tag) = etag {
            if let Ok(value) = HeaderValue::from_str(tag) {
                headers.insert("If-None-Match", value);
            }
        }

        let response = self.send_with_backoff(&request_url, headers).await?;

        if response.status() == StatusCode::NOT_MODIFIED {
            return Ok(StarredPage {
                repos: Vec::new(),
                next_url: None,
                etag: etag.map(str::to_string),
                not_modified: true,
            });
        }

        if !response.status().is_success() {
            return Err(github_http_error(response).await);
        }

        let page_etag = response
            .headers()
            .get("etag")
            .and_then(|v| v.to_str().ok())
            .map(str::to_string);
        let next_url = parse_next_link(response.headers().get(LINK));
        let items: Vec<StarredItem> = response.json().await?;
        let repos = items.into_iter().map(StarredRepo::from).collect();

        Ok(StarredPage {
            repos,
            next_url,
            etag: page_etag,
            not_modified: false,
        })
    }

    pub async fn fetch_readme(&self, owner: &str, repo: &str) -> AppResult<Option<String>> {
        let url = format!("{}/repos/{owner}/{repo}/readme", self.base_url);
        let mut headers = self.auth_headers()?;
        headers.insert(
            ACCEPT,
            HeaderValue::from_static("application/vnd.github+json"),
        );

        let response = self.send_with_backoff(&url, headers).await?;
        if response.status() == StatusCode::NOT_FOUND {
            return Ok(Some(String::new()));
        }
        if !response.status().is_success() {
            return Err(github_http_error(response).await);
        }

        let body: GitHubReadme = response.json().await?;
        let content = body.content.unwrap_or_default();
        let encoding = body
            .encoding
            .unwrap_or_else(|| "base64".into())
            .to_ascii_lowercase();

        // GitHub may return encoding "none" / empty content for missing or exotic READMEs.
        // Treat those as "no excerpt" rather than failing the whole queue.
        if content.trim().is_empty() || encoding == "none" {
            return Ok(Some(String::new()));
        }
        if encoding != "base64" {
            return Ok(Some(String::new()));
        }

        match decode_base64_readme(&content) {
            Ok(decoded) => Ok(Some(strip_readme_noise(&decoded))),
            Err(_) => Ok(Some(String::new())),
        }
    }

    fn auth_headers(&self) -> AppResult<HeaderMap> {
        let mut headers = HeaderMap::new();
        let auth = format!("Bearer {}", self.pat);
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&auth)
                .map_err(|_| AppError::auth("invalid PAT for Authorization header"))?,
        );
        headers.insert("X-GitHub-Api-Version", HeaderValue::from_static(API_VERSION));
        headers.insert(USER_AGENT, HeaderValue::from_static(DEFAULT_USER_AGENT));
        Ok(headers)
    }

    async fn send_with_backoff(&self, url: &str, headers: HeaderMap) -> AppResult<Response> {
        let mut attempt = 0;
        loop {
            let response = self
                .client
                .get(url)
                .headers(headers.clone())
                .send()
                .await?;

            maybe_wait_for_rate_limit(response.headers()).await;

            let status = response.status();
            if matches!(status, StatusCode::FORBIDDEN | StatusCode::TOO_MANY_REQUESTS)
                && attempt < MAX_RETRIES
            {
                let delay = retry_delay(response.headers(), attempt);
                tokio::time::sleep(delay).await;
                attempt += 1;
                continue;
            }

            return Ok(response);
        }
    }
}

pub fn parse_next_link(link_header: Option<&HeaderValue>) -> Option<String> {
    let value = link_header?.to_str().ok()?;
    for part in value.split(',') {
        let part = part.trim();
        let mut segments = part.split(';');
        let url = segments.next()?.trim();
        let is_next = segments.any(|s| s.trim() == "rel=\"next\"");
        if is_next {
            return Some(url.trim_matches('<').trim_matches('>').to_string());
        }
    }
    None
}

pub fn compute_backoff_ms(attempt: u32, retry_after_secs: Option<u64>) -> u64 {
    if let Some(secs) = retry_after_secs {
        return secs.saturating_mul(1000);
    }
    let base = 500u64.saturating_mul(2u64.saturating_pow(attempt));
    let jitter = {
        let mut rng = rand::thread_rng();
        rng.gen_range(0..250)
    };
    base.saturating_add(jitter)
}

fn retry_delay(headers: &HeaderMap, attempt: u32) -> Duration {
    let retry_after = headers
        .get("retry-after")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<u64>().ok());
    Duration::from_millis(compute_backoff_ms(attempt, retry_after))
}

async fn maybe_wait_for_rate_limit(headers: &HeaderMap) {
    let remaining = headers
        .get("x-ratelimit-remaining")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<u32>().ok());
    let reset = headers
        .get("x-ratelimit-reset")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<u64>().ok());

    if let (Some(remaining), Some(reset)) = (remaining, reset) {
        if remaining < RATE_LIMIT_FLOOR {
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            if reset > now {
                let jitter = {
                    let mut rng = rand::thread_rng();
                    rng.gen_range(0..1_000)
                };
                let wait = Duration::from_secs(reset - now) + Duration::from_millis(jitter);
                tokio::time::sleep(wait).await;
            }
        }
    }
}

async fn github_http_error(response: Response) -> AppError {
    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    let detail = extract_message(&body).unwrap_or_else(|| "no error body".to_string());
    AppError::network(format!("HTTP {status}: {detail}"))
}

fn extract_message(body: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(body).ok()?;
    value
        .get("message")
        .and_then(|m| m.as_str())
        .map(str::to_string)
}

pub fn decode_base64_readme(content: &str) -> AppResult<String> {
    use base64::Engine;
    let cleaned: String = content.chars().filter(|c| !c.is_whitespace()).collect();
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(cleaned.as_bytes())
        .map_err(|e| AppError::network(format!("failed to decode README: {e}")))?;
    String::from_utf8(bytes)
        .map_err(|e| AppError::network(format!("README is not valid UTF-8: {e}")))
}

pub fn strip_readme_noise(markdown: &str) -> String {
    let mut out = String::new();
    let mut in_fence = false;
    for line in markdown.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("```") {
            in_fence = !in_fence;
            continue;
        }
        if in_fence {
            continue;
        }
        // Drop badge / align / media chrome that dominates many READMEs.
        if is_readme_noise_line(trimmed) {
            continue;
        }
        let cleaned_line = strip_html_keep_text(line);
        if cleaned_line.trim().is_empty() {
            continue;
        }
        if !out.is_empty() {
            out.push('\n');
        }
        out.push_str(&cleaned_line);
        if out.len() >= 1500 {
            break;
        }
    }
    // Truncation can still split a leftover tag if any slipped through.
    truncate_utf8(&strip_html_keep_text(&out), 1500)
}

fn is_readme_noise_line(trimmed: &str) -> bool {
    let lower = trimmed.to_ascii_lowercase();
    if lower.starts_with("<img")
        || lower.starts_with("<picture")
        || lower.starts_with("<p align")
        || lower.starts_with("<div align")
        || lower.starts_with("<h1 align")
        || lower.starts_with("<br")
        || lower == "<p>"
        || lower == "</p>"
        || lower == "<div>"
        || lower == "</div>"
    {
        return true;
    }
    if lower.contains("shields.io") || (lower.contains("badge") && lower.contains("](")) {
        return true;
    }
    if trimmed.contains("![") && trimmed.contains("](") && lower.contains("badge") {
        return true;
    }
    // Empty anchor wrappers left after badge images are stripped.
    if lower.starts_with("<a ") && !lower.contains('>') {
        return true;
    }
    if lower == "</a>" || (lower.starts_with("<a ") && !contains_visible_anchor_text(trimmed)) {
        return true;
    }
    false
}

fn contains_visible_anchor_text(line: &str) -> bool {
    let text = strip_html_keep_text(line);
    !text.trim().is_empty()
}

/// Remove HTML tags but keep inner text so truncated READMEs don't show raw markup.
pub fn strip_html_keep_text(input: &str) -> String {
    static TAG_RE: OnceLock<Regex> = OnceLock::new();
    static DANGLING_RE: OnceLock<Regex> = OnceLock::new();
    let tag_re = TAG_RE.get_or_init(|| Regex::new(r"(?is)<[^>]+>").expect("html tag regex"));
    let dangling_re =
        DANGLING_RE.get_or_init(|| Regex::new(r"(?is)<[^>]*$").expect("dangling tag regex"));
    let without_complete = tag_re.replace_all(input, "");
    let without_tags = dangling_re.replace_all(&without_complete, "");
    decode_basic_entities(&without_tags)
}

fn decode_basic_entities(input: &str) -> String {
    input
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&apos;", "'")
        .replace("&nbsp;", " ")
}

/// Truncate to at most `max_bytes` without splitting a UTF-8 codepoint.
pub fn truncate_utf8(input: &str, max_bytes: usize) -> String {
    if input.len() <= max_bytes {
        return input.to_string();
    }
    let mut end = max_bytes;
    while end > 0 && !input.is_char_boundary(end) {
        end -= 1;
    }
    input[..end].to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{header, method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[test]
    fn parse_next_link_extracts_url() {
        let value = HeaderValue::from_static(
            "<https://api.github.com/user/starred?page=2>; rel=\"next\", <https://api.github.com/user/starred?page=5>; rel=\"last\"",
        );
        assert_eq!(
            parse_next_link(Some(&value)).as_deref(),
            Some("https://api.github.com/user/starred?page=2")
        );
    }

    #[test]
    fn backoff_honors_retry_after_and_grows() {
        assert_eq!(compute_backoff_ms(0, Some(2)), 2000);
        let first = compute_backoff_ms(0, None);
        let second = compute_backoff_ms(1, None);
        assert!(first >= 500);
        assert!(second >= 1000);
    }

    #[test]
    fn strip_readme_keeps_prose_and_truncates() {
        let md = "Hello\n```\ncode\n```\n![badge](https://img.shields.io/badge/x)\nWorld\n";
        let cleaned = strip_readme_noise(md);
        assert!(cleaned.contains("Hello"));
        assert!(cleaned.contains("World"));
        assert!(!cleaned.contains("code"));
        assert!(!cleaned.contains("badge"));
    }

    #[test]
    fn strip_readme_removes_html_tags_and_dangling_truncation() {
        let md = r#"<p>
<strong>Mobile-first web interface for <a href="https://opencode.ai">OpenCode</a> AI agents.</strong>
</p>
<a href="https://github.com/example/repo/blob/main/LICENSE">
</a>
<a href="https://github.com/example/repo/stargazers">
Quick Start
On first launch, y"#;
        let cleaned = strip_readme_noise(md);
        assert!(cleaned.contains("Mobile-first web interface for OpenCode AI agents."));
        assert!(cleaned.contains("Quick Start"));
        assert!(!cleaned.contains("<p>"));
        assert!(!cleaned.contains("</p>"));
        assert!(!cleaned.contains("<strong>"));
        assert!(!cleaned.contains("<a href"));
        assert!(!cleaned.contains("</a>"));
    }

    #[test]
    fn strip_html_keep_text_handles_incomplete_tag() {
        let s = r#"Hello <a href="https://example.com"#;
        let cleaned = strip_html_keep_text(s);
        assert_eq!(cleaned, "Hello ");
    }

    #[test]
    fn truncate_utf8_does_not_panic_on_multibyte() {
        // '你' is 3 bytes; cutting at a non-boundary used to panic via String::truncate.
        let s = "你好世界".repeat(400);
        let truncated = truncate_utf8(&s, 1500);
        assert!(truncated.len() <= 1500);
        assert!(truncated.is_char_boundary(truncated.len()));
        let _ = strip_readme_noise(&s);
    }

    #[tokio::test]
    async fn readme_encoding_none_returns_empty_excerpt() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/repos/owner/repo/readme"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "content": "",
                "encoding": "none"
            })))
            .expect(1)
            .mount(&server)
            .await;

        let client = GitHubClient::new("test-token", server.uri()).expect("client");
        let excerpt = client
            .fetch_readme("owner", "repo")
            .await
            .expect("fetch")
            .expect("some");
        assert_eq!(excerpt, "");
    }

    #[tokio::test]
    async fn starred_fetch_sends_star_json_accept_header() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/user/starred"))
            .and(query_param("per_page", "100"))
            .and(header("Accept", STAR_ACCEPT))
            .and(header("Authorization", "Bearer test-token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
            .expect(1)
            .mount(&server)
            .await;

        let client = GitHubClient::new("test-token", server.uri()).expect("client");
        let page = client.fetch_starred_page(None, None).await.expect("fetch");
        assert!(page.repos.is_empty());
        assert!(!page.not_modified);
    }

    #[tokio::test]
    async fn etag_304_short_circuits() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/user/starred"))
            .and(header("If-None-Match", "\"abc\""))
            .respond_with(ResponseTemplate::new(304))
            .expect(1)
            .mount(&server)
            .await;

        let client = GitHubClient::new("test-token", server.uri()).expect("client");
        let page = client
            .fetch_starred_page(None, Some("\"abc\""))
            .await
            .expect("fetch");
        assert!(page.not_modified);
        assert!(page.repos.is_empty());
    }

    #[tokio::test]
    async fn second_sync_with_etag_makes_at_most_one_starred_request() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/user/starred"))
            .and(header("Accept", STAR_ACCEPT))
            .respond_with(ResponseTemplate::new(304))
            .expect(1)
            .mount(&server)
            .await;

        let client = GitHubClient::new("test-token", server.uri()).expect("client");
        let page = client
            .fetch_starred_page(None, Some("\"etag-1\""))
            .await
            .expect("fetch");
        assert!(page.not_modified);
        // wiremock expect(1) asserts ≤1 starred request for the 304 fast path
    }
}
