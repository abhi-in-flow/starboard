use crate::error::{AppError, AppResult};
use crate::models::GitHubUser;

const USER_AGENT: &str = "Starboard/0.1 (+https://github.com/brocode/starboard)";

pub async fn validate_pat(pat: &str) -> AppResult<GitHubUser> {
    let trimmed = pat.trim();
    if trimmed.is_empty() {
        return Err(AppError::auth("PAT cannot be empty"));
    }

    let client = reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .build()?;

    let response = client
        .get("https://api.github.com/user")
        .header("Authorization", format!("Bearer {trimmed}"))
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .send()
        .await?;

    let status = response.status();
    if status.is_success() {
        let user = response.json::<GitHubUser>().await?;
        return Ok(user);
    }

    let body = response.text().await.unwrap_or_default();
    let message = extract_github_message(&body).unwrap_or_else(|| {
        if status.as_u16() == 401 {
            "Bad credentials".to_string()
        } else {
            format!("GitHub returned HTTP {status}")
        }
    });

    Err(AppError::auth(message))
}

fn extract_github_message(body: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(body).ok()?;
    value
        .get("message")
        .and_then(|m| m.as_str())
        .map(str::to_string)
}
