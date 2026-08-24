use tauri::State;

use crate::error::{AppError, AppResult};
use crate::models::{ReviewCounts, SetRepoReviewRequest};
use crate::services::review;
use crate::services::store::{self, DbState};

#[tauri::command]
pub fn get_review_counts(state: State<'_, DbState>) -> AppResult<ReviewCounts> {
    store::with_conn(&state, review::review_counts)
}

#[tauri::command]
pub fn set_repo_review(state: State<'_, DbState>, request: SetRepoReviewRequest) -> AppResult<()> {
    if request.repo_id <= 0 {
        return Err(AppError::new("validation_error", "repo_id is required"));
    }
    store::with_conn(&state, |conn| review::set_repo_review(conn, &request))
}
