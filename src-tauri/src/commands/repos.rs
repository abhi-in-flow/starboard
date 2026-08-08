use tauri::{AppHandle, State};

use crate::error::AppResult;
use crate::models::{
    CategoryNode, LibraryFacets, ListReposRequest, RepoDetail, RepoFilters, RepoListResult,
    SearchReposRequest,
};
use crate::services::{categories, repos, search};
use crate::services::store::{self, DbState};

#[tauri::command]
pub fn list_repos(
    state: State<'_, DbState>,
    request: ListReposRequest,
) -> AppResult<RepoListResult> {
    store::with_conn(&state, |conn| repos::list_repos(conn, request))
}

#[tauri::command]
pub async fn search_repos(
    app: AppHandle,
    request: SearchReposRequest,
) -> AppResult<RepoListResult> {
    search::search_repos_async(&app, request).await
}

#[tauri::command]
pub fn get_repo(state: State<'_, DbState>, id: i64) -> AppResult<RepoDetail> {
    store::with_conn(&state, |conn| repos::get_repo(conn, id))
}

#[tauri::command]
pub fn get_library_facets(
    state: State<'_, DbState>,
    filters: Option<RepoFilters>,
) -> AppResult<LibraryFacets> {
    store::with_conn(&state, |conn| repos::library_facets(conn, filters))
}

#[tauri::command]
pub fn list_categories(state: State<'_, DbState>) -> AppResult<Vec<CategoryNode>> {
    store::with_conn(&state, categories::list_category_tree)
}
