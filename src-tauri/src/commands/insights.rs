use tauri::State;

use crate::error::AppResult;
use crate::models::{
    InsightsDashboard, InsightsDateRange, InsightsRequest, LibraryExport, SharePoint,
};
use crate::services::insights;
use crate::services::store::{self, DbState};

#[tauri::command]
pub fn get_insights(
    state: State<'_, DbState>,
    request: InsightsRequest,
) -> AppResult<InsightsDashboard> {
    store::with_conn(&state, |conn| insights::get_insights(conn, request))
}

#[tauri::command]
pub fn get_interest_drift_drilldown(
    state: State<'_, DbState>,
    range: Option<InsightsDateRange>,
    category: String,
    utc_offset_minutes: Option<i32>,
) -> AppResult<Vec<SharePoint>> {
    if category.trim().is_empty() {
        return Err(crate::error::AppError::new(
            "validation_error",
            "category is required",
        ));
    }
    store::with_conn(&state, |conn| {
        insights::interest_drift_drilldown(conn, range, category.trim(), utc_offset_minutes)
    })
}

#[tauri::command]
pub fn get_library_export(state: State<'_, DbState>) -> AppResult<LibraryExport> {
    store::with_conn(&state, insights::export_library)
}

#[tauri::command]
pub fn write_library_export(
    state: State<'_, DbState>,
    path: String,
    format: String,
) -> AppResult<()> {
    let format = format.to_ascii_lowercase();
    let _validated = insights::validate_export_path(path.trim(), Some(&format))?;
    let content = store::with_conn(&state, |conn| {
        let export = insights::export_library(conn)?;
        match format.as_str() {
            "markdown" | "md" => Ok(export.markdown),
            "json" => Ok(export.json),
            other => Err(crate::error::AppError::new(
                "validation_error",
                format!("unknown export format: {other}"),
            )),
        }
    })?;
    insights::write_export_file(path.trim(), &content)
}
